use std::{fs, path::Path};

use catan_bots::{bot::BotPolicy, greedy::GreedyAgent, lazy::LazyAgent, random::RandomAgent};
use catan_core::gameplay::{
    game::{run::RunOptions, state::SetupGameState},
    primitives::player::PlayerId,
};

use crate::{
    config::{self, FieldConfig, InitialStateConfig, MatchConfig, SeatConfig},
    persistence::PersistenceObserver,
    snapshot,
    sync_host::{Seat, SyncGameHost, bot_seat},
};

pub fn load_config(path: &Path) -> Result<MatchConfig, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {err}", path.display()))?;
    let mut config: MatchConfig = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {err}", path.display()))?;
    config::resolve_paths(&mut config, path.parent().unwrap_or_else(|| Path::new(".")));
    Ok(config)
}

pub fn run_match(config: MatchConfig) -> Result<(), String> {
    if config.players.is_empty() {
        return Err("config must contain at least one player".to_owned());
    }

    let seats = build_seats(&config.players)?;
    let options = RunOptions {
        max_turns: config.limits.max_turns,
        max_invalid_actions: config.limits.max_invalid_actions,
        ..RunOptions::default()
    };
    let engine = build_initial_engine(&config, config.players.len(), options)?;

    match &config.dice {
        crate::config::DiceConfig::Random => {}
    }

    let mut host = SyncGameHost::from_engine(engine, seats);
    if !config.observers.is_empty() {
        return Err(
            "terminal/remote observers are hosted by catan-remote, not catan-runtime".to_owned(),
        );
    }
    if let Some(observer) = PersistenceObserver::from_config(&config.persistence)
        .map_err(|err| format!("failed to initialize persistence: {err}"))?
    {
        host.add_observer(Box::new(observer));
    }
    host.start();
    let result = host.run_to_result();
    log::info!("match result: {result:?}");
    Ok(())
}

pub fn build_initial_engine(
    config: &MatchConfig,
    player_count: usize,
    options: RunOptions,
) -> Result<catan_core::gameplay::game::engine::GameEngine, String> {
    match &config.initial {
        InitialStateConfig::Fresh => {
            let init = build_initial_state(&config.field, player_count)?;
            Ok(catan_core::gameplay::game::engine::GameEngine::from_init(
                init, options,
            ))
        }
        InitialStateConfig::Snapshot { path } => {
            let loaded = snapshot::load_checkpoint(path)
                .map_err(|err| format!("failed to load snapshot {}: {err}", path.display()))?;
            let snapshot = loaded.snapshot;
            let snapshot_players = snapshot.state.table().players.count();
            if snapshot_players != player_count {
                return Err(format!(
                    "snapshot has {snapshot_players} players but config declares {player_count}"
                ));
            }
            Ok(
                catan_core::gameplay::game::engine::GameEngine::from_snapshot(
                    snapshot,
                    loaded.board,
                    options,
                ),
            )
        }
    }
}

pub fn build_seats(players: &[SeatConfig]) -> Result<Vec<Box<dyn Seat>>, String> {
    players
        .iter()
        .enumerate()
        .map(|(id, player)| {
            let player_id = PlayerId::try_from(id).map_err(|err| err.to_string())?;
            match player {
                SeatConfig::Lazy => Ok(bot_seat(
                    Box::new(LazyAgent::new(player_id)) as Box<dyn BotPolicy>
                )),
                SeatConfig::Greedy => Ok(bot_seat(
                    Box::new(GreedyAgent::new(player_id)) as Box<dyn BotPolicy>
                )),
                SeatConfig::Random => Ok(bot_seat(
                    Box::new(RandomAgent::new(player_id)) as Box<dyn BotPolicy>
                )),
                SeatConfig::Remote => Err(
                    "remote/TUI player seats are hosted by catan-remote, not catan-runtime"
                        .to_owned(),
                ),
            }
        })
        .collect()
}

pub fn build_bot_seat(
    player: &SeatConfig,
    player_id: PlayerId,
) -> Result<Option<Box<dyn Seat>>, String> {
    match player {
        SeatConfig::Lazy => Ok(Some(bot_seat(
            Box::new(LazyAgent::new(player_id)) as Box<dyn BotPolicy>
        ))),
        SeatConfig::Greedy => Ok(Some(bot_seat(
            Box::new(GreedyAgent::new(player_id)) as Box<dyn BotPolicy>
        ))),
        SeatConfig::Random => Ok(Some(bot_seat(
            Box::new(RandomAgent::new(player_id)) as Box<dyn BotPolicy>
        ))),
        SeatConfig::Remote => Ok(None),
    }
}

pub fn build_initial_state(
    config: &FieldConfig,
    player_count: usize,
) -> Result<SetupGameState, String> {
    match config {
        FieldConfig::Default => {
            let field = catan_core::gameplay::field::state::FieldBuildParam {
                n_players: player_count,
                ..Default::default()
            };
            Ok(SetupGameState::new(field))
        }
        FieldConfig::LayoutRef { path } => {
            let arrangement = catan_core::gameplay::field::ser::arrangement_from_json(path)
                .ok_or_else(|| format!("failed to read field layout {}", path.display()))?;
            Ok(SetupGameState::new(
                catan_core::gameplay::field::state::FieldBuildParam {
                    n_players: player_count,
                    arrangement,
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use catan_core::gameplay::game::{engine::GameEngine, run::RunOptions, state::SetupGameState};

    use crate::{
        config::{
            DiceConfig, FieldConfig, InitialStateConfig, LimitsConfig, LoggingConfig, MatchConfig,
            PersistenceConfig, SeatConfig,
        },
        host::{build_initial_engine, build_initial_state},
        snapshot::SnapshotStore,
    };

    #[test]
    fn default_field_uses_configured_player_count() {
        for player_count in [1, 2, 3, 4, 6] {
            let init = build_initial_state(&FieldConfig::Default, player_count).unwrap();

            assert_eq!(init.board.n_players, player_count);
            assert_eq!(init.players.count(), player_count);
            assert_eq!(init.builds.players().len(), player_count);
        }
    }

    #[test]
    fn snapshot_initial_engine_preserves_pending_decisions() {
        let dir = unique_test_dir();
        let mut engine = GameEngine::from_init(SetupGameState::default(), RunOptions::default());
        engine.start().expect("engine should start");
        let mut store = SnapshotStore::new_in(dir.clone()).unwrap();
        let snapshot_dir = store.write_checkpoint(&engine).unwrap();
        let config = MatchConfig {
            players: vec![
                SeatConfig::Lazy,
                SeatConfig::Lazy,
                SeatConfig::Lazy,
                SeatConfig::Lazy,
            ],
            observers: Vec::new(),
            initial: InitialStateConfig::Snapshot { path: snapshot_dir },
            field: FieldConfig::Default,
            dice: DiceConfig::default(),
            limits: LimitsConfig::default(),
            logging: LoggingConfig::default(),
            persistence: PersistenceConfig::default(),
        };

        let loaded = build_initial_engine(&config, config.players.len(), RunOptions::default())
            .expect("snapshot should load into an engine");

        assert!(loaded.is_started());
        assert_eq!(loaded.pending_decisions().count(), 1);

        std::fs::remove_dir_all(dir).unwrap();
    }

    fn unique_test_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "rusty-catan-host-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
