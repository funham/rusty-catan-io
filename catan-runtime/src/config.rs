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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlayerConfig {
    Cli,
    Lazy,
    Greedy,
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
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_turns: default_max_turns(),
        }
    }
}

fn default_max_turns() -> Option<u64> {
    Some(500)
}
