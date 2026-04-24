use crate::error::{AppError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompletedAssets {
    pub symbol: bool,
    pub footprint: bool,
    pub model_3d: bool,
}

impl CompletedAssets {
    pub const fn all() -> Self {
        Self {
            symbol: true,
            footprint: true,
            model_3d: true,
        }
    }

    pub const fn none() -> Self {
        Self {
            symbol: false,
            footprint: false,
            model_3d: false,
        }
    }

    pub fn covers(self, required: Self) -> bool {
        (!required.symbol || self.symbol)
            && (!required.footprint || self.footprint)
            && (!required.model_3d || self.model_3d)
    }

    pub fn union_assign(&mut self, other: Self) {
        self.symbol |= other.symbol;
        self.footprint |= other.footprint;
        self.model_3d |= other.model_3d;
    }

    fn encode(self) -> &'static str {
        match (self.symbol, self.footprint, self.model_3d) {
            (true, true, true) => "sfm",
            (true, true, false) => "sf",
            (true, false, true) => "sm",
            (false, true, true) => "fm",
            (true, false, false) => "s",
            (false, true, false) => "f",
            (false, false, true) => "m",
            (false, false, false) => "",
        }
    }

    fn decode(value: &str) -> Option<Self> {
        let mut assets = Self::none();
        for ch in value.chars() {
            match ch {
                's' => assets.symbol = true,
                'f' => assets.footprint = true,
                'm' => assets.model_3d = true,
                _ => return None,
            }
        }

        if assets == Self::none() {
            None
        } else {
            Some(assets)
        }
    }
}

pub struct CheckpointManager {
    path: PathBuf,
    completed: Mutex<HashMap<String, CompletedAssets>>,
}

impl CheckpointManager {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let completed = load_checkpoint_file(&path)?;
        Ok(Self {
            path,
            completed: Mutex::new(completed),
        })
    }

    pub fn completed_assets(&self) -> HashMap<String, CompletedAssets> {
        self.completed
            .lock()
            .expect("checkpoint mutex poisoned")
            .clone()
    }

    pub fn record_completed_ids(&self, lcsc_ids: &[String], assets: CompletedAssets) -> Result<()> {
        if lcsc_ids.is_empty() || assets == CompletedAssets::none() {
            return Ok(());
        }

        let mut completed = self.completed.lock().expect("checkpoint mutex poisoned");
        for lcsc_id in lcsc_ids {
            completed.entry(lcsc_id.clone()).or_default().union_assign(assets);
        }

        write_checkpoint_file(&self.path, &completed)
    }
}

fn load_checkpoint_file(path: &Path) -> Result<HashMap<String, CompletedAssets>> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let mut completed: HashMap<String, CompletedAssets> = HashMap::new();
            for line in content.lines().map(str::trim).filter(|line| !line.is_empty()) {
                let (lcsc_id, assets) = parse_checkpoint_line(line);
                completed.entry(lcsc_id).or_default().union_assign(assets);
            }
            Ok(completed)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(error) => Err(AppError::io_context("read checkpoint", path, error)),
    }
}

fn parse_checkpoint_line(line: &str) -> (String, CompletedAssets) {
    if let Some((lcsc_id, encoded_assets)) = line.split_once('\t')
        && let Some(assets) = CompletedAssets::decode(encoded_assets)
    {
        return (lcsc_id.trim().to_string(), assets);
    }

    (line.to_string(), CompletedAssets::all())
}

fn write_checkpoint_file(
    path: &Path,
    completed: &HashMap<String, CompletedAssets>,
) -> Result<()> {
    let mut entries: Vec<_> = completed.iter().collect();
    entries.sort_by(|left, right| left.0.cmp(right.0));

    let mut output = String::new();
    for (lcsc_id, assets) in entries {
        if *assets == CompletedAssets::none() {
            continue;
        }
        output.push_str(lcsc_id);
        output.push('\t');
        output.push_str(assets.encode());
        output.push('\n');
    }

    std::fs::write(path, output).map_err(|error| AppError::io_context("write checkpoint", path, error))
}

#[cfg(test)]
mod tests {
    use super::{CheckpointManager, CompletedAssets};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "nlbn_checkpoint_tests_{}_{}_{}.txt",
            name,
            std::process::id(),
            stamp
        ))
    }

    #[test]
    fn loads_existing_checkpoint_ids() {
        let path = test_path("load");
        fs::write(&path, "C1\nC2\ts\n\nC3\tfm\n").unwrap();

        let checkpoint = CheckpointManager::load(&path).unwrap();
        let completed = checkpoint.completed_assets();

        assert_eq!(completed.get("C1"), Some(&CompletedAssets::all()));
        assert_eq!(
            completed.get("C2"),
            Some(&CompletedAssets {
                symbol: true,
                footprint: false,
                model_3d: false,
            })
        );
        assert_eq!(
            completed.get("C3"),
            Some(&CompletedAssets {
                symbol: false,
                footprint: true,
                model_3d: true,
            })
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn record_completed_ids_merges_asset_sets() {
        let path = test_path("append");
        let checkpoint = CheckpointManager::load(&path).unwrap();

        checkpoint
            .record_completed_ids(
                &["C1".to_string(), "C2".to_string()],
                CompletedAssets {
                    symbol: true,
                    footprint: false,
                    model_3d: false,
                },
            )
            .unwrap();
        checkpoint
            .record_completed_ids(
                &["C2".to_string(), "C3".to_string()],
                CompletedAssets {
                    symbol: false,
                    footprint: true,
                    model_3d: true,
                },
            )
            .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.lines().collect::<Vec<_>>(), vec!["C1\ts", "C2\tsfm", "C3\tfm"]);

        let _ = fs::remove_file(path);
    }
}
