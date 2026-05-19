use std::{
    io,
    path::{Path, PathBuf},
};

use catan_core::gameplay::game::{engine::GameEngine, run::RunOptions};

use crate::{persistence::read_journal_suffix, snapshot::load_latest_checkpoint_at_or_before};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewindTarget {
    EventSeq(u64),
}

pub fn rewind_engine(
    persistence_dir: &Path,
    target: RewindTarget,
    options: RunOptions,
) -> io::Result<GameEngine> {
    let target_seq = resolve_target_seq(target);
    replay_to_event_seq(persistence_dir, target_seq, options)
}

pub fn replay_to_event_seq(
    persistence_dir: &Path,
    target_seq: u64,
    options: RunOptions,
) -> io::Result<GameEngine> {
    let checkpoint_root = persistence_dir.join("checkpoints");
    let loaded = load_latest_checkpoint_at_or_before(&checkpoint_root, target_seq)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no checkpoint available"))?;
    let checkpoint_seq = loaded.checkpoint_seq;
    let mut engine = GameEngine::from_snapshot(loaded.snapshot, loaded.board, options);
    for record in read_journal_suffix(
        &persistence_dir.join("journal.jsonl"),
        checkpoint_seq,
        target_seq,
    )? {
        engine
            .apply_event(&record.event)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{err:?}")))?;
    }
    Ok(engine)
}

fn resolve_target_seq(target: RewindTarget) -> u64 {
    match target {
        RewindTarget::EventSeq(seq) => seq,
    }
}

pub fn persistence_dir(path: impl Into<PathBuf>) -> PathBuf {
    path.into()
}
