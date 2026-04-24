use crate::error::{AppError, Result};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct CheckpointManager {
    path: PathBuf,
    completed_ids: Mutex<HashSet<String>>,
}

impl CheckpointManager {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let completed_ids = load_checkpoint_file(&path)?;
        Ok(Self {
            path,
            completed_ids: Mutex::new(completed_ids),
        })
    }

    pub fn completed_ids(&self) -> HashSet<String> {
        self.completed_ids
            .lock()
            .expect("checkpoint mutex poisoned")
            .clone()
    }

    pub fn append_completed_ids(&self, lcsc_ids: &[String]) -> Result<()> {
        if lcsc_ids.is_empty() {
            return Ok(());
        }

        let mut completed_ids = self
            .completed_ids
            .lock()
            .expect("checkpoint mutex poisoned");

        let mut pending_ids = Vec::new();
        for lcsc_id in lcsc_ids {
            if completed_ids.insert(lcsc_id.clone()) {
                pending_ids.push(lcsc_id.as_str());
            }
        }

        if pending_ids.is_empty() {
            return Ok(());
        }

        append_checkpoint_file(&self.path, &pending_ids)
    }
}

fn load_checkpoint_file(path: &Path) -> Result<HashSet<String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
        Err(error) => Err(AppError::io_context("read checkpoint", path, error)),
    }
}

fn append_checkpoint_file(path: &Path, lcsc_ids: &[&str]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| AppError::io_context("open checkpoint for append", path, error))?;

    for lcsc_id in lcsc_ids {
        writeln!(file, "{}", lcsc_id)
            .map_err(|error| AppError::io_context("append to checkpoint", path, error))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::CheckpointManager;
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
        fs::write(&path, "C1\nC2\n\nC3\n").unwrap();

        let checkpoint = CheckpointManager::load(&path).unwrap();

        let ids = checkpoint.completed_ids();
        assert!(ids.contains("C1"));
        assert!(ids.contains("C2"));
        assert!(ids.contains("C3"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn append_completed_ids_deduplicates_entries() {
        let path = test_path("append");
        let checkpoint = CheckpointManager::load(&path).unwrap();

        checkpoint
            .append_completed_ids(&["C1".to_string(), "C2".to_string()])
            .unwrap();
        checkpoint
            .append_completed_ids(&["C2".to_string(), "C3".to_string()])
            .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.lines().collect::<Vec<_>>(), vec!["C1", "C2", "C3"]);

        let _ = fs::remove_file(path);
    }
}
