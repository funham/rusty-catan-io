use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use catan_core::gameplay::game::{
    event::{EventVisibility, GameEvent},
    output::GameOutput,
};
use serde::{Deserialize, Serialize};

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
    event: &'a GameEvent,
    visibility: &'a EventVisibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedJournalRecord {
    pub schema: String,
    pub seq: u64,
    pub event: GameEvent,
    pub visibility: EventVisibility,
}

pub fn read_journal_suffix(
    journal_path: &Path,
    checkpoint_seq: u64,
    target_seq: u64,
) -> io::Result<Vec<PersistedJournalRecord>> {
    let file = File::open(journal_path)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let record: PersistedJournalRecord =
            serde_json::from_str(&line).map_err(io::Error::other)?;
        if record.schema != "rusty-catan.journal.v1" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported journal schema {}", record.schema),
            ));
        }
        if record.seq > checkpoint_seq && record.seq <= target_seq {
            records.push(record);
        }
    }

    Ok(records)
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
        if self.event_seq == 0
            && let Some(policy) = self.checkpoints.as_mut()
            && let Err(err) = policy.store.write_checkpoint_at(frame.engine, 0)
        {
            log::warn!("failed to write initial checkpoint: {err}");
        }
        self.event_seq += 1;
        let record = JournalRecord {
            schema: "rusty-catan.journal.v1".to_owned(),
            seq: self.event_seq,
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
            && let Err(err) = policy
                .store
                .write_checkpoint_at(frame.engine, self.event_seq)
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
        output::{GameEventRecord, GameOutput},
        run::RunOptions,
        state::SetupGameState,
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
        let init = SetupGameState::default();
        let engine = GameEngine::from_init(init.clone(), RunOptions::default());
        let state = engine.table();
        let index = GameIndex::rebuild_table(state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let output = GameOutput::Event(GameEventRecord {
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
        assert!(journal.contains("\"seq\":1"));
        assert!(!journal.contains("\"tx_id\""));
        assert!(journal.contains("\"event\":\"GameStarted\""));
        assert!(journal.contains("\"visibility\":\"Public\""));
        assert!(dir.join("checkpoints").join("snapshot-000001").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn journal_records_can_be_loaded_for_replay_suffix() {
        let dir = unique_test_dir();
        let mut observer =
            PersistenceObserver::from_config(&PersistenceConfig::JournalOnly { dir: dir.clone() })
                .unwrap()
                .unwrap();
        let init = SetupGameState::default();
        let engine = GameEngine::from_init(init.clone(), RunOptions::default());
        let state = engine.table();
        let index = GameIndex::rebuild_table(state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };

        for _ in 0..3 {
            let output = GameOutput::Event(GameEventRecord {
                event: GameEvent::GameStarted,
                visibility: EventVisibility::Public,
            });
            observer.on_output(ObserverFrame {
                output: &output,
                factory: &factory,
                engine: &engine,
            });
        }

        drop(observer);

        let records = super::read_journal_suffix(&dir.join("journal.jsonl"), 1, 3).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].seq, 2);
        assert_eq!(records[1].seq, 3);
        assert!(
            records
                .iter()
                .all(|record| matches!(record.event, GameEvent::GameStarted))
        );

        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_test_dir() -> std::path::PathBuf {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "rusty-catan-persistence-test-{}-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
