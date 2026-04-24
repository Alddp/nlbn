use crate::error::{AppError, KicadError, Result};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Default)]
struct SymbolLibrarySession {
    content: String,
    dirty: bool,
    component_names: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    Written(PathBuf),
    Skipped(PathBuf),
}

impl WriteOutcome {
    pub fn was_written(&self) -> bool {
        matches!(self, Self::Written(_))
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Written(path) | Self::Skipped(path) => path.as_path(),
        }
    }

    pub fn into_path(self) -> PathBuf {
        match self {
            Self::Written(path) | Self::Skipped(path) => path,
        }
    }
}

pub struct LibraryManager {
    output_path: PathBuf,
    lib_name: String,
    overwrite: bool,
    write_lock: Mutex<()>,
    symbol_sessions: Mutex<HashMap<PathBuf, SymbolLibrarySession>>,
}

impl LibraryManager {
    pub fn new(output_path: &Path) -> Self {
        Self::with_overwrite(output_path, false)
    }

    pub fn with_overwrite(output_path: &Path, overwrite: bool) -> Self {
        let lib_name = output_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("nlbn")
            .to_string();
        Self {
            output_path: output_path.to_path_buf(),
            lib_name,
            overwrite,
            write_lock: Mutex::new(()),
            symbol_sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn lib_name(&self) -> &str {
        &self.lib_name
    }

    pub fn overwrite_enabled(&self) -> bool {
        self.overwrite
    }

    pub fn should_write_file(&self, path: &Path) -> bool {
        self.should_write_file_with_overwrite(path, self.overwrite)
    }

    pub fn should_write_file_with_overwrite(&self, path: &Path, overwrite: bool) -> bool {
        overwrite || !path.exists()
    }

    /// Create necessary output directories
    pub fn create_directories(&self) -> Result<()> {
        // Create main output directory
        fs::create_dir_all(&self.output_path).map_err(|error| {
            AppError::io_context("create output directory", &self.output_path, error)
        })?;

        // Create .pretty directory for footprints
        let pretty_dir = self.output_path.join(format!("{}.pretty", self.lib_name));
        fs::create_dir_all(&pretty_dir).map_err(|error| {
            AppError::io_context("create footprint directory", &pretty_dir, error)
        })?;

        // Create .3dshapes directory for 3D models
        let shapes_dir = self.output_path.join(format!("{}.3dshapes", self.lib_name));
        fs::create_dir_all(&shapes_dir).map_err(|error| {
            AppError::io_context("create 3D model directory", &shapes_dir, error)
        })?;

        Ok(())
    }

    /// Check if a component exists in the library file
    /// Note: This should only be called within a lock if used for write decisions
    pub fn component_exists(&self, lib_path: &Path, component_name: &str) -> Result<bool> {
        if let Ok(sessions) = self.symbol_sessions.lock()
            && let Some(session) = sessions.get(lib_path)
        {
            return Ok(session.component_names.contains(component_name));
        }

        if !lib_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(lib_path)
            .map_err(|error| AppError::io_context("read symbol library", lib_path, error))?;
        Ok(collect_component_names(&content).contains(component_name))
    }

    pub fn stage_or_update_component(
        &self,
        lib_path: &Path,
        component_name: &str,
        component_data: &str,
        overwrite: bool,
    ) -> Result<()> {
        let _lock = self.write_lock.lock().unwrap();
        self.stage_or_update_component_locked(lib_path, component_name, component_data, overwrite)
    }

    /// Add or update a component in the library file (thread-safe)
    pub fn add_or_update_component(
        &self,
        lib_path: &Path,
        component_name: &str,
        component_data: &str,
        overwrite: bool,
    ) -> Result<()> {
        let _lock = self.write_lock.lock().unwrap();
        self.stage_or_update_component_locked(lib_path, component_name, component_data, overwrite)?;
        self.flush_symbol_libraries_locked()
    }

    pub fn flush_symbol_libraries(&self) -> Result<()> {
        let _lock = self.write_lock.lock().unwrap();
        self.flush_symbol_libraries_locked()
    }

    fn stage_or_update_component_locked(
        &self,
        lib_path: &Path,
        component_name: &str,
        component_data: &str,
        overwrite: bool,
    ) -> Result<()> {
        let mut sessions = self.symbol_sessions.lock().unwrap();
        let session = get_or_load_symbol_session(&mut sessions, lib_path)?;
        let exists = session.component_names.contains(component_name);

        if exists && overwrite {
            session.content =
                update_component_in_content(&session.content, component_name, component_data)?;
            session.dirty = true;
        } else if !exists {
            session.content = add_component_to_content(&session.content, component_data);
            session.component_names.insert(component_name.to_string());
            session.dirty = true;
        }

        Ok(())
    }

    fn flush_symbol_libraries_locked(&self) -> Result<()> {
        let mut sessions = self.symbol_sessions.lock().unwrap();
        for (path, session) in sessions.iter_mut() {
            if !session.dirty {
                continue;
            }
            Self::atomic_write(path, session.content.as_bytes(), 64 * 1024)
                .map_err(|error| AppError::io_context("write symbol library", path, error))?;
            log::info!("Wrote symbol library: {}", path.display());
            session.dirty = false;
        }
        Ok(())
    }

    /// Add a component to the library file
    pub fn add_component(&self, lib_path: &Path, component_data: &str) -> Result<()> {
        let _lock = self.write_lock.lock().unwrap();
        let current_content = load_symbol_library_content(lib_path)?;
        let new_content = add_component_to_content(&current_content, component_data);
        Self::atomic_write(lib_path, new_content.as_bytes(), 64 * 1024)
            .map_err(|error| AppError::io_context("write symbol library", lib_path, error))?;
        self.replace_symbol_session_content(lib_path, new_content);
        Ok(())
    }

    /// Update an existing component in the library file
    pub fn update_component(
        &self,
        lib_path: &Path,
        component_name: &str,
        new_data: &str,
    ) -> Result<()> {
        let _lock = self.write_lock.lock().unwrap();
        let content = load_symbol_library_content(lib_path)?;
        let new_content = update_component_in_content(&content, component_name, new_data)?;
        Self::atomic_write(lib_path, new_content.as_bytes(), 64 * 1024)
            .map_err(|error| AppError::io_context("write symbol library", lib_path, error))?;
        self.replace_symbol_session_content(lib_path, new_content);
        Ok(())
    }

    /// Atomic write: write to temp file with buffered I/O, then rename
    fn atomic_write(
        path: &Path,
        data: &[u8],
        buf_size: usize,
    ) -> std::result::Result<(), std::io::Error> {
        let tmp_path = path.with_extension("tmp");
        {
            let file = fs::File::create(&tmp_path)?;
            let mut writer = BufWriter::with_capacity(buf_size, file);
            writer.write_all(data)?;
            writer.flush()?;
        }
        fs::rename(&tmp_path, path)?;
        Ok(())
    }

    fn write_output_file(
        &self,
        path: &Path,
        data: &[u8],
        buf_size: usize,
        kind: &str,
        overwrite: bool,
    ) -> Result<WriteOutcome> {
        if !self.should_write_file_with_overwrite(path, overwrite) {
            log::info!("Skipping existing {}: {}", kind, path.display());
            return Ok(WriteOutcome::Skipped(path.to_path_buf()));
        }

        Self::atomic_write(path, data, buf_size)
            .map_err(|error| AppError::io_context("write output file", path, error))?;
        log::info!("Wrote {}: {}", kind, path.display());
        Ok(WriteOutcome::Written(path.to_path_buf()))
    }

    /// Write a footprint file
    pub fn write_footprint(&self, footprint_name: &str, data: &str) -> Result<PathBuf> {
        Ok(self
            .write_footprint_with_status(footprint_name, data, false)?
            .into_path())
    }

    /// Write a footprint file and report whether it changed
    pub fn write_footprint_with_status(
        &self,
        footprint_name: &str,
        data: &str,
        overwrite: bool,
    ) -> Result<WriteOutcome> {
        let footprint_path = self.get_footprint_path(footprint_name);

        self.write_output_file(
            &footprint_path,
            data.as_bytes(),
            32 * 1024,
            "footprint",
            overwrite,
        )
    }

    /// Write 3D model files
    pub fn write_3d_model(
        &self,
        model_name: &str,
        wrl_data: &str,
        step_data: &[u8],
    ) -> Result<(PathBuf, PathBuf)> {
        let wrl_path = self
            .write_wrl_model_with_status(model_name, wrl_data, false)?
            .into_path();

        let step_path = self.get_step_path(model_name);
        if !step_data.is_empty() {
            self.write_step_model_with_status(model_name, step_data, false)?;
        }

        Ok((wrl_path, step_path))
    }

    /// Write only VRML model (when STEP is not available)
    pub fn write_wrl_model(&self, model_name: &str, wrl_data: &str) -> Result<PathBuf> {
        Ok(self
            .write_wrl_model_with_status(model_name, wrl_data, false)?
            .into_path())
    }

    /// Write only VRML model and report whether it changed
    pub fn write_wrl_model_with_status(
        &self,
        model_name: &str,
        wrl_data: &str,
        overwrite: bool,
    ) -> Result<WriteOutcome> {
        let wrl_path = self.get_wrl_path(model_name);

        self.write_output_file(
            &wrl_path,
            wrl_data.as_bytes(),
            256 * 1024,
            "VRML model",
            overwrite,
        )
    }

    /// Write only STEP model
    pub fn write_step_model(&self, model_name: &str, step_data: &[u8]) -> Result<PathBuf> {
        Ok(self
            .write_step_model_with_status(model_name, step_data, false)?
            .into_path())
    }

    /// Write only STEP model and report whether it changed
    pub fn write_step_model_with_status(
        &self,
        model_name: &str,
        step_data: &[u8],
        overwrite: bool,
    ) -> Result<WriteOutcome> {
        let step_path = self.get_step_path(model_name);

        self.write_output_file(&step_path, step_data, 256 * 1024, "STEP model", overwrite)
    }

    /// Get the path for a WRL model file
    pub fn get_wrl_path(&self, model_name: &str) -> PathBuf {
        self.output_path
            .join(format!("{}.3dshapes", self.lib_name))
            .join(format!("{}.wrl", model_name))
    }

    /// Get the path for a footprint file
    pub fn get_footprint_path(&self, footprint_name: &str) -> PathBuf {
        self.output_path
            .join(format!("{}.pretty", self.lib_name))
            .join(format!("{}.kicad_mod", footprint_name))
    }

    /// Get the path for a STEP model file
    pub fn get_step_path(&self, model_name: &str) -> PathBuf {
        self.output_path
            .join(format!("{}.3dshapes", self.lib_name))
            .join(format!("{}.step", model_name))
    }

    /// Get the symbol library path
    pub fn get_symbol_lib_path(&self) -> PathBuf {
        self.output_path
            .join(format!("{}.kicad_sym", self.lib_name))
    }

    fn replace_symbol_session_content(&self, lib_path: &Path, content: String) {
        if let Ok(mut sessions) = self.symbol_sessions.lock() {
            let component_names = collect_component_names(&content);
            sessions.insert(
                lib_path.to_path_buf(),
                SymbolLibrarySession {
                    content,
                    dirty: false,
                    component_names,
                },
            );
        }
    }
}

fn get_or_load_symbol_session<'a>(
    sessions: &'a mut HashMap<PathBuf, SymbolLibrarySession>,
    lib_path: &Path,
) -> Result<&'a mut SymbolLibrarySession> {
    if !sessions.contains_key(lib_path) {
        let content = load_symbol_library_content(lib_path)?;
        let component_names = collect_component_names(&content);
        sessions.insert(
            lib_path.to_path_buf(),
            SymbolLibrarySession {
                content,
                dirty: false,
                component_names,
            },
        );
    }

    Ok(sessions
        .get_mut(lib_path)
        .expect("symbol session should exist after insertion"))
}

fn load_symbol_library_content(lib_path: &Path) -> Result<String> {
    if lib_path.exists() {
        fs::read_to_string(lib_path)
            .map_err(|error| AppError::io_context("read symbol library", lib_path, error))
    } else {
        Ok(String::new())
    }
}

fn collect_component_names(content: &str) -> HashSet<String> {
    let mut names = HashSet::new();

    if let Ok(v6) = Regex::new(r#"\(symbol\s+"([^"]+)""#) {
        for captures in v6.captures_iter(content) {
            if let Some(name) = captures.get(1) {
                names.insert(name.as_str().to_string());
            }
        }
    }

    names
}

fn add_component_to_content(existing_content: &str, component_data: &str) -> String {
    let mut content = if existing_content.is_empty() {
        String::from("(kicad_symbol_lib\n  (version 20211014)\n  (generator nlbn)")
    } else {
        existing_content
            .trim_end()
            .trim_end_matches(')')
            .to_string()
    };

    content.push('\n');
    content.push_str(component_data);

    if component_data.contains("(symbol") {
        content.push('\n');
        content.push(')');
    }
    content.push('\n');

    content
}

fn update_component_in_content(
    content: &str,
    component_name: &str,
    new_data: &str,
) -> Result<String> {
    let search = format!(r#"(symbol "{}""#, component_name);
    if let Some(start) = content.find(&search) {
        let block_start = find_v6_block_start(content, start);
        if let Some(block_end) = find_matching_paren_end(content, start) {
            let mut new_content = String::with_capacity(content.len());
            new_content.push_str(&content[..block_start]);
            if block_start > 0 && !content[..block_start].ends_with('\n') {
                new_content.push('\n');
            }
            new_content.push_str(new_data);
            new_content.push_str(&content[block_end..]);
            return Ok(new_content);
        }
    }

    Err(
        KicadError::SymbolExport(format!("Component {} not found in library", component_name))
            .into(),
    )
}

fn find_v6_block_start(content: &str, symbol_start: usize) -> usize {
    let mut block_start = symbol_start;
    while block_start > 0 && content.as_bytes()[block_start - 1] == b' ' {
        block_start -= 1;
    }
    if block_start > 0 && content.as_bytes()[block_start - 1] == b'\n' {
        block_start -= 1;
    }
    block_start
}

fn find_matching_paren_end(content: &str, start: usize) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in content[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }

            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }

            continue;
        }

        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{LibraryManager, add_component_to_content, update_component_in_content};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "nlbn_library_tests_{}_{}_{}",
            name,
            std::process::id(),
            stamp
        ))
    }

    fn symbol_block(name: &str) -> String {
        format!("  (symbol \"{}\")", name)
    }

    fn symbol_block_with_property(name: &str, value: &str) -> String {
        format!(
            "  (symbol \"{}\"\n    (property \"Value\" \"{}\"))",
            name, value
        )
    }

    #[test]
    fn staged_symbol_updates_are_flushed_together() {
        let root = test_root("stage_flush");
        let output_dir = root.join("demo-lib");
        let manager = LibraryManager::new(&output_dir);
        manager.create_directories().unwrap();

        let lib_path = manager.get_symbol_lib_path();
        manager
            .stage_or_update_component(&lib_path, "A", &symbol_block("A"), false)
            .unwrap();
        manager
            .stage_or_update_component(&lib_path, "B", &symbol_block("B"), false)
            .unwrap();

        assert!(!lib_path.exists());

        manager.flush_symbol_libraries().unwrap();

        let content = fs::read_to_string(&lib_path).unwrap();
        assert!(content.contains("(symbol \"A\")"));
        assert!(content.contains("(symbol \"B\")"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn staged_symbol_overwrite_updates_buffered_content() {
        let root = test_root("stage_overwrite");
        let output_dir = root.join("demo-lib");
        let manager = LibraryManager::new(&output_dir);
        manager.create_directories().unwrap();

        let lib_path = manager.get_symbol_lib_path();
        manager
            .stage_or_update_component(&lib_path, "A", &symbol_block("A"), false)
            .unwrap();
        manager.flush_symbol_libraries().unwrap();

        manager
            .stage_or_update_component(
                &lib_path,
                "A",
                "  (symbol \"A\"\n    (property \"Value\" \"Updated\"))",
                true,
            )
            .unwrap();
        manager.flush_symbol_libraries().unwrap();

        let content = fs::read_to_string(&lib_path).unwrap();
        assert!(content.contains("Updated"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn immediate_symbol_update_still_writes_file() {
        let root = test_root("immediate_write");
        let output_dir = root.join("demo-lib");
        let manager = LibraryManager::new(&output_dir);
        manager.create_directories().unwrap();

        let lib_path = manager.get_symbol_lib_path();
        manager
            .add_or_update_component(&lib_path, "A", &symbol_block("A"), false)
            .unwrap();

        assert!(lib_path.exists());
        let content = fs::read_to_string(&lib_path).unwrap();
        assert!(content.contains("(symbol \"A\")"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn overwrite_handles_parentheses_inside_symbol_properties() {
        let initial = add_component_to_content(
            "",
            &symbol_block_with_property("A", "Before (draft) release"),
        );
        let updated = update_component_in_content(
            &initial,
            "A",
            &symbol_block_with_property("A", "After (rev B) release"),
        )
        .unwrap();

        assert!(updated.contains("After (rev B) release"));
        assert!(!updated.contains("Before (draft) release"));
    }

    #[test]
    fn overwrite_handles_escaped_quotes_inside_symbol_properties() {
        let initial =
            add_component_to_content("", &symbol_block_with_property("A", "Before \\\"A\\\""));
        let updated = update_component_in_content(
            &initial,
            "A",
            &symbol_block_with_property("A", "After \\\"B\\\" (final)"),
        )
        .unwrap();

        assert!(updated.contains("After \\\"B\\\" (final)"));
        assert!(!updated.contains("Before \\\"A\\\""));
    }
}
