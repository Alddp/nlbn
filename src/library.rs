use crate::error::{KicadError, Result};
use regex::Regex;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static SYMBOL_WRITE_LOCK: Mutex<()> = Mutex::new(());
static DEFAULT_OVERWRITE: AtomicBool = AtomicBool::new(false);

pub fn set_default_overwrite(overwrite: bool) {
    DEFAULT_OVERWRITE.store(overwrite, Ordering::Relaxed);
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
}

impl LibraryManager {
    pub fn new(output_path: &Path) -> Self {
        Self::with_overwrite(output_path, DEFAULT_OVERWRITE.load(Ordering::Relaxed))
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
        }
    }

    pub fn lib_name(&self) -> &str {
        &self.lib_name
    }

    pub fn overwrite_enabled(&self) -> bool {
        self.overwrite
    }

    pub fn should_write_file(&self, path: &Path) -> bool {
        self.overwrite || !path.exists()
    }

    /// Create necessary output directories
    pub fn create_directories(&self) -> Result<()> {
        // Create main output directory
        fs::create_dir_all(&self.output_path).map_err(KicadError::Io)?;

        // Create .pretty directory for footprints
        let pretty_dir = self.output_path.join(format!("{}.pretty", self.lib_name));
        fs::create_dir_all(&pretty_dir).map_err(KicadError::Io)?;

        // Create .3dshapes directory for 3D models
        let shapes_dir = self.output_path.join(format!("{}.3dshapes", self.lib_name));
        fs::create_dir_all(&shapes_dir).map_err(KicadError::Io)?;

        Ok(())
    }

    /// Check if a component exists in the library file
    /// Note: This should only be called within a lock if used for write decisions
    pub fn component_exists(&self, lib_path: &Path, component_name: &str) -> Result<bool> {
        if !lib_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(lib_path).map_err(KicadError::Io)?;

        // Check for v6 format
        let v6_pattern = format!(r#"\(symbol\s+"{}""#, regex::escape(component_name));
        if let Ok(re) = Regex::new(&v6_pattern) {
            if re.is_match(&content) {
                return Ok(true);
            }
        }

        // Check for v5 format
        let v5_pattern = format!(r"DEF\s+{}\s+", regex::escape(component_name));
        if let Ok(re) = Regex::new(&v5_pattern) {
            if re.is_match(&content) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Add or update a component in the library file (thread-safe)
    pub fn add_or_update_component(
        &self,
        lib_path: &Path,
        component_name: &str,
        component_data: &str,
        overwrite: bool,
    ) -> Result<()> {
        // Lock to prevent concurrent writes and check-then-act race conditions
        let _lock = SYMBOL_WRITE_LOCK.lock().unwrap();

        // Check if component exists (within lock to prevent TOCTOU)
        let exists = if lib_path.exists() {
            let content = fs::read_to_string(lib_path).map_err(KicadError::Io)?;

            let v6_pattern = format!(r#"\(symbol\s+"{}""#, regex::escape(component_name));
            if let Ok(re) = Regex::new(&v6_pattern) {
                re.is_match(&content)
            } else {
                false
            }
        } else {
            false
        };

        if exists && overwrite {
            // Update existing component
            self.update_component_internal(lib_path, component_name, component_data)?;
        } else if !exists {
            // Add new component
            self.add_component_internal(lib_path, component_data)?;
        }
        // If exists and !overwrite, do nothing

        Ok(())
    }

    /// Internal add component (assumes lock is held)
    fn add_component_internal(&self, lib_path: &Path, component_data: &str) -> Result<()> {
        let mut content = if lib_path.exists() {
            let existing = fs::read_to_string(lib_path).map_err(KicadError::Io)?;
            existing.trim_end().trim_end_matches(')').to_string()
        } else {
            if component_data.contains("(symbol") {
                String::from("(kicad_symbol_lib\n  (version 20211014)\n  (generator nlbn)")
            } else {
                String::from("EESchema-LIBRARY Version 2.4\n#encoding utf-8")
            }
        };

        content.push('\n');
        content.push_str(component_data);

        if component_data.contains("(symbol") {
            content.push('\n');
            content.push(')');
        }
        content.push('\n');

        fs::write(lib_path, content).map_err(KicadError::Io)?;

        Ok(())
    }

    /// Internal update component (assumes lock is held)
    fn update_component_internal(
        &self,
        lib_path: &Path,
        component_name: &str,
        new_data: &str,
    ) -> Result<()> {
        let content = fs::read_to_string(lib_path).map_err(KicadError::Io)?;

        // Try v6 format: find symbol block by matching parentheses
        let search = format!(r#"(symbol "{}""#, component_name);
        if let Some(start) = content.find(&search) {
            // Walk back to consume leading whitespace/newline before (symbol
            let mut block_start = start;
            while block_start > 0 && content.as_bytes()[block_start - 1] == b' ' {
                block_start -= 1;
            }
            if block_start > 0 && content.as_bytes()[block_start - 1] == b'\n' {
                block_start -= 1;
            }

            // Count parentheses from start to find the matching close
            let mut depth = 0;
            let mut block_end = start;
            for (i, ch) in content[start..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            block_end = start + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if block_end > start {
                let mut new_content = String::with_capacity(content.len());
                new_content.push_str(&content[..block_start]);
                new_content.push('\n');
                new_content.push_str(new_data);
                new_content.push_str(&content[block_end..]);
                fs::write(lib_path, &new_content).map_err(KicadError::Io)?;
                return Ok(());
            }
        }

        // Try v5 format
        let v5_start = format!("DEF {} ", component_name);
        if let Some(start) = content.find(&v5_start) {
            if let Some(end_offset) = content[start..].find("ENDDEF") {
                let block_end = start + end_offset + "ENDDEF".len();
                // Skip trailing newline
                let block_end = if content[block_end..].starts_with('\n') {
                    block_end + 1
                } else {
                    block_end
                };
                let mut new_content = String::with_capacity(content.len());
                new_content.push_str(&content[..start]);
                new_content.push_str(new_data);
                new_content.push_str(&content[block_end..]);
                fs::write(lib_path, &new_content).map_err(KicadError::Io)?;
                return Ok(());
            }
        }

        Err(
            KicadError::SymbolExport(format!("Component {} not found in library", component_name))
                .into(),
        )
    }

    /// Add a component to the library file
    pub fn add_component(&self, lib_path: &Path, component_data: &str) -> Result<()> {
        // Lock to prevent concurrent writes to the same symbol library file
        let _lock = SYMBOL_WRITE_LOCK.lock().unwrap();

        let mut content = if lib_path.exists() {
            // Read existing file and remove the closing parenthesis
            let existing = fs::read_to_string(lib_path).map_err(KicadError::Io)?;
            // Remove trailing ')' and whitespace
            existing.trim_end().trim_end_matches(')').to_string()
        } else {
            // Create new library file with header (v6 format with proper formatting)
            if component_data.contains("(symbol") {
                // v6 format - match Python's formatting exactly
                String::from("(kicad_symbol_lib\n  (version 20211014)\n  (generator nlbn)")
            } else {
                // v5 format
                String::from("EESchema-LIBRARY Version 2.4\n#encoding utf-8")
            }
        };

        // Append component
        content.push('\n');
        content.push_str(component_data);

        // Add closing parenthesis for v6 format
        if component_data.contains("(symbol") {
            content.push('\n');
            content.push(')');
        }
        content.push('\n');

        fs::write(lib_path, content).map_err(KicadError::Io)?;

        Ok(())
    }

    /// Update an existing component in the library file
    pub fn update_component(
        &self,
        lib_path: &Path,
        component_name: &str,
        new_data: &str,
    ) -> Result<()> {
        // Lock to prevent concurrent writes to the same symbol library file
        let _lock = SYMBOL_WRITE_LOCK.lock().unwrap();

        let content = fs::read_to_string(lib_path).map_err(KicadError::Io)?;

        // Try v6 format: find symbol block by matching parentheses
        let search = format!(r#"(symbol "{}""#, component_name);
        if let Some(start) = content.find(&search) {
            let block_start = content[..start].rfind('(').unwrap_or(start);
            let mut depth = 0;
            let mut block_end = block_start;
            for (i, ch) in content[block_start..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            block_end = block_start + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if block_end > block_start {
                let mut new_content = String::with_capacity(content.len());
                new_content.push_str(&content[..block_start]);
                new_content.push_str(new_data);
                new_content.push_str(&content[block_end..]);
                fs::write(lib_path, &new_content).map_err(KicadError::Io)?;
                return Ok(());
            }
        }

        // Try v5 format
        let v5_start = format!("DEF {} ", component_name);
        if let Some(start) = content.find(&v5_start) {
            if let Some(end_offset) = content[start..].find("ENDDEF") {
                let block_end = start + end_offset + "ENDDEF".len();
                let block_end = if content[block_end..].starts_with('\n') {
                    block_end + 1
                } else {
                    block_end
                };
                let mut new_content = String::with_capacity(content.len());
                new_content.push_str(&content[..start]);
                new_content.push_str(new_data);
                new_content.push_str(&content[block_end..]);
                fs::write(lib_path, &new_content).map_err(KicadError::Io)?;
                return Ok(());
            }
        }

        Err(
            KicadError::SymbolExport(format!("Component {} not found in library", component_name))
                .into(),
        )
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
    ) -> Result<WriteOutcome> {
        if !self.should_write_file(path) {
            log::info!("Skipping existing {}: {}", kind, path.display());
            return Ok(WriteOutcome::Skipped(path.to_path_buf()));
        }

        Self::atomic_write(path, data, buf_size).map_err(KicadError::Io)?;
        log::info!("Wrote {}: {}", kind, path.display());
        Ok(WriteOutcome::Written(path.to_path_buf()))
    }

    /// Write a footprint file
    pub fn write_footprint(&self, footprint_name: &str, data: &str) -> Result<PathBuf> {
        Ok(self
            .write_footprint_with_status(footprint_name, data)?
            .into_path())
    }

    /// Write a footprint file and report whether it changed
    pub fn write_footprint_with_status(
        &self,
        footprint_name: &str,
        data: &str,
    ) -> Result<WriteOutcome> {
        let footprint_path = self.get_footprint_path(footprint_name);

        self.write_output_file(&footprint_path, data.as_bytes(), 32 * 1024, "footprint")
    }

    /// Write 3D model files
    pub fn write_3d_model(
        &self,
        model_name: &str,
        wrl_data: &str,
        step_data: &[u8],
    ) -> Result<(PathBuf, PathBuf)> {
        let wrl_path = self
            .write_wrl_model_with_status(model_name, wrl_data)?
            .into_path();

        let step_path = self.get_step_path(model_name);
        if !step_data.is_empty() {
            self.write_step_model_with_status(model_name, step_data)?;
        }

        Ok((wrl_path, step_path))
    }

    /// Write only VRML model (when STEP is not available)
    pub fn write_wrl_model(&self, model_name: &str, wrl_data: &str) -> Result<PathBuf> {
        Ok(self
            .write_wrl_model_with_status(model_name, wrl_data)?
            .into_path())
    }

    /// Write only VRML model and report whether it changed
    pub fn write_wrl_model_with_status(
        &self,
        model_name: &str,
        wrl_data: &str,
    ) -> Result<WriteOutcome> {
        let wrl_path = self.get_wrl_path(model_name);

        self.write_output_file(&wrl_path, wrl_data.as_bytes(), 256 * 1024, "VRML model")
    }

    /// Write only STEP model
    pub fn write_step_model(&self, model_name: &str, step_data: &[u8]) -> Result<PathBuf> {
        Ok(self
            .write_step_model_with_status(model_name, step_data)?
            .into_path())
    }

    /// Write only STEP model and report whether it changed
    pub fn write_step_model_with_status(
        &self,
        model_name: &str,
        step_data: &[u8],
    ) -> Result<WriteOutcome> {
        let step_path = self.get_step_path(model_name);

        self.write_output_file(&step_path, step_data, 256 * 1024, "STEP model")
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
    pub fn get_symbol_lib_path(&self, v5: bool) -> PathBuf {
        if v5 {
            self.output_path.join(format!("{}.lib", self.lib_name))
        } else {
            self.output_path
                .join(format!("{}.kicad_sym", self.lib_name))
        }
    }
}
