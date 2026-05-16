use std::{
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

use catan_core::gameplay::game::{
    event::{EventVisibility, GameEvent},
    output::GameOutput,
};
use serde::Serialize;

use crate::{
    config::PersistenceConfig,
    snapshot::SnapshotStore,
    sync_host::{ObserverFrame, OutputObserver},
};

pub struct PersistenceObserver {
    event_seq: u64,
    journal: File,
    checkpoints: Option<CheckpointPolicy>,
}

struct CheckpointPolicy {
    every: u64,
    store: SnapshotStore,
}

#[derive(Serialize)]
struct JournalRecord<'a> {
    schema: String,
    seq: u64,
    tx_id: u64,
    event: &'a GameEvent,
    visibility: &'a EventVisibility,
}

impl PersistenceObserver {
    pub fn from_config(config: &PersistenceConfig) -> io::Result<Option<Self>> {
        match config {
            PersistenceConfig::Off => Ok(None),
            PersistenceConfig::JournalOnly { dir } => Self::new(dir.clone(), None).map(Some),
            PersistenceConfig::JournalWithCheckpoints {
                dir,
                checkpoint_every,
            } => Self::new(dir.clone(), Some(*checkpoint_every)).map(Some),
        }
    }

    fn new(dir: PathBuf, checkpoint_every: Option<u64>) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let journal = File::create(dir.join("journal.jsonl"))?;
        let checkpoints = match checkpoint_every {
            Some(every) if every > 0 => Some(CheckpointPolicy {
                every,
                store: SnapshotStore::new_in(dir.join("checkpoints"))?,
            }),
            _ => None,
        };
        Ok(Self {
            event_seq: 0,
            journal,
            checkpoints,
        })
    }
}

impl OutputObserver for PersistenceObserver {
    fn on_output(&mut self, frame: ObserverFrame<'_>) {
        let GameOutput::Event(record) = frame.output else {
            return;
        };
        self.event_seq += 1;
        let record = JournalRecord {
            schema: "rusty-catan.journal.v1".to_owned(),
            seq: self.event_seq,
            tx_id: record.tx_id,
            event: &record.event,
            visibility: &record.visibility,
        };
        if let Err(err) = serde_json::to_writer(&mut self.journal, &record)
            .and_then(|_| self.journal.write_all(b"\n").map_err(serde_json::Error::io))
        {
            log::warn!("failed to write journal event {}: {err}", self.event_seq);
        }

        if let Some(policy) = self.checkpoints.as_mut()
            && self.event_seq % policy.every == 0
            && let Err(err) = policy.store.write_checkpoint(frame.engine)
        {
            log::warn!(
                "failed to write checkpoint at event {}: {err}",
                self.event_seq
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use catan_core::gameplay::game::{
        engine::GameEngine,
        event::GameEvent,
        index::GameIndex,
        init::GameInitializationState,
        output::{GameEventRecord, GameOutput},
        run::RunOptions,
        view::{ContextFactory, VisibilityConfig},
    };

    use super::*;

    #[test]
    fn persistence_off_builds_no_observer() {
        assert!(
            PersistenceObserver::from_config(&PersistenceConfig::Off)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn journal_with_checkpoints_writes_event_and_checkpoint() {
        let dir = unique_test_dir();
        let mut observer =
            PersistenceObserver::from_config(&PersistenceConfig::JournalWithCheckpoints {
                dir: dir.clone(),
                checkpoint_every: 1,
            })
            .unwrap()
            .unwrap();
        let init = GameInitializationState::default();
        let engine = GameEngine::from_init(init.clone(), RunOptions::default());
        let state = engine.state();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let output = GameOutput::Event(GameEventRecord {
            tx_id: 42,
            event: GameEvent::GameStarted,
            visibility: EventVisibility::Public,
        });

        observer.on_output(ObserverFrame {
            output: &output,
            factory: &factory,
            engine: &engine,
        });

        let journal = fs::read_to_string(dir.join("journal.jsonl")).unwrap();
        assert!(journal.contains("\"schema\":\"rusty-catan.journal.v1\""));
        assert!(journal.contains("\"tx_id\":42"));
        assert!(journal.contains("\"event\":\"GameStarted\""));
        assert!(journal.contains("\"visibility\":\"Public\""));
        assert!(dir.join("checkpoints").join("snapshot-000001").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_test_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "rusty-catan-persistence-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
