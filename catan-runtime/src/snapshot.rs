use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use catan_core::gameplay::{
    field::state::{BoardLayout, FieldBuildParam},
    game::engine::{GameEngine, GameEngineSnapshot},
};
use serde::{Deserialize, Serialize};

pub struct SnapshotStore {
    root: PathBuf,
    next_snapshot: u64,
}

#[derive(Serialize, Deserialize)]
struct SnapshotManifest {
    schema: String,
    snapshot_id: u64,
    checkpoint_seq: u64,
    layout_ref: String,
    state_ref: String,
    layout_hash: String,
    state_hash: String,
}

pub struct LoadedCheckpoint {
    pub path: PathBuf,
    pub checkpoint_seq: u64,
    pub snapshot: GameEngineSnapshot,
    pub board: Arc<BoardLayout>,
}

impl SnapshotStore {
    pub fn new() -> io::Result<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_secs();
        Self::new_in(
            Path::new("target")
                .join("snapshots")
                .join(format!("rusty-catan-host-{timestamp}")),
        )
    }

    pub fn new_in(root: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            next_snapshot: 1,
        })
    }

    pub fn write_checkpoint(&mut self, engine: &GameEngine) -> io::Result<PathBuf> {
        self.write_checkpoint_at(engine, 0)
    }

    pub fn write_checkpoint_at(
        &mut self,
        engine: &GameEngine,
        checkpoint_seq: u64,
    ) -> io::Result<PathBuf> {
        let snapshot_id = self.next_snapshot;
        let dir = self.root.join(format!("snapshot-{snapshot_id:06}"));
        fs::create_dir_all(&dir)?;

        let state = engine.state();
        let engine_snapshot = engine.snapshot();
        let layout_path = dir.join("layout.json");
        let state_path = dir.join("state.json");
        write_pretty_json(&layout_path, &state.board.arrangement)?;
        write_pretty_json(&state_path, &engine_snapshot)?;

        let manifest = SnapshotManifest {
            schema: "rusty-catan.snapshot.v1".to_owned(),
            snapshot_id,
            checkpoint_seq,
            layout_ref: "layout.json".to_owned(),
            state_ref: "state.json".to_owned(),
            layout_hash: stable_json_hash(&state.board.arrangement)?,
            state_hash: stable_json_hash(&engine_snapshot)?,
        };
        write_pretty_json(&dir.join("manifest.json"), &manifest)?;

        self.next_snapshot += 1;
        Ok(dir)
    }
}

pub fn load_checkpoint(dir: &Path) -> io::Result<LoadedCheckpoint> {
    let manifest: SnapshotManifest = read_json(&dir.join("manifest.json"))?;
    if manifest.schema != "rusty-catan.snapshot.v1" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported snapshot schema {}", manifest.schema),
        ));
    }

    let layout_path = dir.join(&manifest.layout_ref);
    let state_path = dir.join(&manifest.state_ref);
    let arrangement = catan_core::gameplay::field::ser::arrangement_from_json(&layout_path)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to read snapshot layout {}", layout_path.display()),
            )
        })?;
    let snapshot: GameEngineSnapshot = read_json(&state_path)?;
    if snapshot.schema != "rusty-catan.engine-snapshot.v1" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported engine snapshot schema {}", snapshot.schema),
        ));
    }
    verify_hash_value(&arrangement, &manifest.layout_hash)?;
    verify_hash_value(&snapshot, &manifest.state_hash)?;

    let board = Arc::new(BoardLayout::new(FieldBuildParam {
        n_players: snapshot.state.players.count(),
        arrangement,
    }));
    Ok(LoadedCheckpoint {
        path: dir.to_path_buf(),
        checkpoint_seq: manifest.checkpoint_seq,
        snapshot,
        board,
    })
}

pub fn load_latest_checkpoint_at_or_before(
    root: &Path,
    target_seq: u64,
) -> io::Result<Option<LoadedCheckpoint>> {
    let mut best: Option<LoadedCheckpoint> = None;

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let candidate = match load_checkpoint(&entry.path()) {
            Ok(candidate) => candidate,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err),
        };
        if candidate.checkpoint_seq > target_seq {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|current| candidate.checkpoint_seq > current.checkpoint_seq)
        {
            best = Some(candidate);
        }
    }

    Ok(best)
}

fn write_pretty_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, value).map_err(io::Error::other)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> io::Result<T> {
    let file = File::open(path)?;
    serde_json::from_reader(file).map_err(io::Error::other)
}

fn stable_json_hash(value: &impl Serialize) -> io::Result<String> {
    let raw = serde_json::to_vec(value).map_err(io::Error::other)?;
    Ok(format!("fnv1a64:{:016x}", fnv1a64(&raw)))
}

fn verify_hash_value(value: &impl Serialize, expected: &str) -> io::Result<()> {
    let actual = stable_json_hash(value)?;
    if actual == expected {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("snapshot hash mismatch: expected {expected}, got {actual}"),
        ))
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::fs;

    use catan_core::gameplay::game::{
        engine::GameEngine, init::GameInitializationState, run::RunOptions,
    };

    use super::{SnapshotStore, load_checkpoint};

    #[test]
    fn checkpoint_store_writes_directory_snapshot() {
        let dir = unique_test_dir();
        let engine =
            GameEngine::from_init(GameInitializationState::default(), RunOptions::default());
        let mut store = SnapshotStore::new_in(dir.clone()).unwrap();

        let snapshot_dir = store.write_checkpoint(&engine).unwrap();

        assert!(snapshot_dir.join("manifest.json").exists());
        assert!(snapshot_dir.join("layout.json").exists());
        assert!(snapshot_dir.join("state.json").exists());

        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(snapshot_dir.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["schema"], "rusty-catan.snapshot.v1");
        assert_eq!(manifest["layout_ref"], "layout.json");
        assert_eq!(manifest["checkpoint_seq"], 0);
        assert!(
            manifest["layout_hash"]
                .as_str()
                .unwrap()
                .starts_with("fnv1a64:")
        );

        let state_raw = fs::read_to_string(snapshot_dir.join("state.json")).unwrap();
        assert!(!state_raw.contains("\"_p\""));
        assert!(!state_raw.contains("\"board\""));
        serde_json::from_str::<catan_core::gameplay::game::engine::GameEngineSnapshot>(&state_raw)
            .unwrap();

        let loaded = load_checkpoint(&snapshot_dir).unwrap();
        assert_eq!(loaded.snapshot.schema, "rusty-catan.engine-snapshot.v1");
        assert_eq!(loaded.board.n_players, 4);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn checkpoint_store_finds_latest_checkpoint_at_or_before_sequence() {
        let dir = unique_test_dir();
        let engine =
            GameEngine::from_init(GameInitializationState::default(), RunOptions::default());
        let mut store = SnapshotStore::new_in(dir.clone()).unwrap();
        store.write_checkpoint_at(&engine, 5).unwrap();
        let second = store.write_checkpoint_at(&engine, 10).unwrap();
        store.write_checkpoint_at(&engine, 20).unwrap();

        let loaded = super::load_latest_checkpoint_at_or_before(&dir, 12)
            .unwrap()
            .expect("checkpoint at seq 10 should be selected");

        assert_eq!(loaded.checkpoint_seq, 10);
        assert_eq!(loaded.path, second);

        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_test_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "rusty-catan-host-snapshot-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
