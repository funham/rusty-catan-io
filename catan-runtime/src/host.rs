use std::{fs, path::Path};

use catan_agents::{bot::BotPolicy, greedy::GreedyAgent, lazy::LazyAgent, random::RandomAgent};
use catan_core::gameplay::game::{init::GameInitializationState, run::RunOptions};

use crate::{
    config::{FieldConfig, MatchConfig, PlayerConfig},
    sync_host::{Seat, SyncGameHost, bot_seat},
};

pub fn load_config(path: &Path) -> Result<MatchConfig, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {err}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {err}", path.display()))
}

pub fn run_match(config: MatchConfig) -> Result<(), String> {
    if config.players.is_empty() {
        return Err("config must contain at least one player".to_owned());
    }

    if !config.observers.is_empty() {
        return Err(
            "event-driven match runner does not yet support observer seats; use player seats only"
                .to_owned(),
        );
    }

    let seats = build_seats(&config.players)?;
    let init_state = build_initial_state(&config.field, config.players.len());

    match &config.dice {
        crate::config::DiceConfig::Random => {}
    }

    let mut host = SyncGameHost::new(
        init_state,
        seats,
        RunOptions {
            max_turns: config.limits.max_turns,
            max_invalid_actions: config.limits.max_invalid_actions,
            ..RunOptions::default()
        },
    );
    host.start();
    let result = host.run_to_result();
    log::info!("match result: {result:?}");
    Ok(())
}

fn build_seats(players: &[PlayerConfig]) -> Result<Vec<Box<dyn Seat>>, String> {
    players
        .iter()
        .enumerate()
        .map(|(id, player)| match player {
            PlayerConfig::Lazy => Ok(bot_seat(Box::new(LazyAgent::new(id)) as Box<dyn BotPolicy>)),
            PlayerConfig::Greedy => Ok(bot_seat(
                Box::new(GreedyAgent::new(id)) as Box<dyn BotPolicy>
            )),
            PlayerConfig::Random => Ok(bot_seat(
                Box::new(RandomAgent::new(id)) as Box<dyn BotPolicy>
            )),
            PlayerConfig::Cli => Err(
                "event-driven match runner does not yet support interactive CLI player seats"
                    .to_owned(),
            ),
        })
        .collect()
}

fn build_initial_state(config: &FieldConfig, player_count: usize) -> GameInitializationState {
    match config {
        FieldConfig::Default => {
            let mut field = catan_core::gameplay::field::state::FieldBuildParam::default();
            field.n_players = player_count;
            GameInitializationState::new(field)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{FieldConfig, PlayerConfig},
        host::{build_initial_state, build_seats},
    };

    #[test]
    fn cli_players_are_rejected_until_event_seat_exists() {
        let err = match build_seats(&[PlayerConfig::Cli]) {
            Ok(_) => panic!("CLI player seats should be rejected"),
            Err(err) => err,
        };

        assert!(err.contains("interactive CLI player seats"));
    }

    #[test]
    fn default_field_uses_configured_player_count() {
        for player_count in [1, 2, 3, 4, 6] {
            let init = build_initial_state(&FieldConfig::Default, player_count);

            assert_eq!(init.board.n_players, player_count);
            assert_eq!(init.players.count(), player_count);
            assert_eq!(init.builds.players().len(), player_count);
        }
    }
}
