use std::{fs, path::Path};

use catan_agents::{
    cli_agent::{CliAgent, SharedTerminalUi},
    lazy_agent::LazyAgent,
};
use catan_core::{
    GameInitializer,
    agent::Agent,
    gameplay::game::init::GameInitializationState,
    math::dice::{DiceRoller, RandomDiceRoller},
};

use crate::config::{DiceConfig, FieldConfig, MatchConfig, PlayerConfig};

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

    let terminal = SharedTerminalUi::default();
    let agents = build_agents(&config.players, terminal);
    let mut dice = build_dice(&config.dice);
    let init_state = build_initial_state(&config.field);
    let mut runner = GameInitializer::new(init_state, agents).init();

    runner.run(dice.as_mut());
    Ok(())
}

fn build_agents(players: &[PlayerConfig], terminal: SharedTerminalUi) -> Vec<Box<dyn Agent>> {
    players
        .iter()
        .enumerate()
        .map(|(id, player)| match player {
            PlayerConfig::Cli => Box::new(CliAgent::new(id, terminal.clone())) as Box<dyn Agent>,
            PlayerConfig::Lazy => Box::new(LazyAgent::new(id)) as Box<dyn Agent>,
        })
        .collect()
}

fn build_dice(config: &DiceConfig) -> Box<dyn DiceRoller> {
    match config {
        DiceConfig::Random => Box::new(RandomDiceRoller::new()),
    }
}

fn build_initial_state(config: &FieldConfig) -> GameInitializationState {
    match config {
        FieldConfig::Default => GameInitializationState::default(),
    }
}
