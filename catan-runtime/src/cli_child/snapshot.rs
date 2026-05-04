//! Snapshot file writer for the snapshot observer role.

use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use catan_core::gameplay::game::state::GameState;

pub(crate) struct SnapshotWriter {
    dir: PathBuf,
    next_snapshot: u64,
}

impl SnapshotWriter {
    pub(crate) fn new() -> io::Result<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_secs();
        Self::new_in(
            Path::new("target")
                .join("snapshots")
                .join(format!("rusty-catan-{timestamp}")),
        )
    }

    pub(crate) fn new_in(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            next_snapshot: 1,
        })
    }

    pub(crate) fn write(&mut self, state: &GameState) -> io::Result<PathBuf> {
        let path = self.dir.join(format!("{:06}.json", self.next_snapshot));
        let file = File::create(&path)?;
        serde_json::to_writer_pretty(file, state).map_err(io::Error::other)?;
        self.next_snapshot += 1;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use catan_core::gameplay::game::{init::GameInitializationState, state::GameState};

    use super::SnapshotWriter;

    #[test]
    fn snapshot_writer_creates_numbered_json_files() {
        let dir = unique_test_dir();
        let state = GameInitializationState::default().finish();
        let mut writer = SnapshotWriter::new_in(dir.clone()).unwrap();

        let first = writer.write(&state).unwrap();
        let second = writer.write(&state).unwrap();

        assert_eq!(first.file_name().unwrap(), "000001.json");
        assert_eq!(second.file_name().unwrap(), "000002.json");

        let raw = fs::read_to_string(first).unwrap();
        serde_json::from_str::<GameState>(&raw).unwrap();

        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_test_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rusty-catan-snapshot-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
