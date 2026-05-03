use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MatchConfig {
    pub players: Vec<PlayerConfig>,
    #[serde(default)]
    pub observers: Vec<ObserverConfig>,
    #[serde(default)]
    pub field: FieldConfig,
    #[serde(default)]
    pub dice: DiceConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlayerConfig {
    Cli,
    Lazy,
    Greedy,
    Random,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObserverConfig {
    CliSpectator,
    CliPlayer { player_id: usize },
    CliOmniscient,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldConfig {
    #[default]
    Default,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiceConfig {
    #[default]
    Random,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LimitsConfig {
    #[serde(default = "default_max_turns")]
    pub max_turns: Option<u64>,
    #[serde(default = "default_max_invalid_actions")]
    pub max_invalid_actions: Option<u64>,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_turns: default_max_turns(),
            max_invalid_actions: default_max_invalid_actions(),
        }
    }
}

fn default_max_turns() -> Option<u64> {
    Some(500)
}

fn default_max_invalid_actions() -> Option<u64> {
    Some(10)
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_logging_enabled")]
    pub enabled: bool,
    #[serde(default = "default_logging_directory")]
    pub directory: PathBuf,
    #[serde(default = "default_logging_file_prefix")]
    pub file_prefix: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: default_logging_enabled(),
            directory: default_logging_directory(),
            file_prefix: default_logging_file_prefix(),
        }
    }
}

fn default_logging_enabled() -> bool {
    true
}

fn default_logging_directory() -> PathBuf {
    PathBuf::from("target/catan-logs")
}

fn default_logging_file_prefix() -> String {
    "rusty-catan".to_owned()
}
