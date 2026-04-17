use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MatchConfig {
    pub players: Vec<PlayerConfig>,
    #[serde(default = "default_displays")]
    pub displays: Vec<DisplayConfig>,
    #[serde(default)]
    pub field: FieldConfig,
    #[serde(default)]
    pub dice: DiceConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlayerConfig {
    Cli,
    Lazy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DisplayConfig {
    Ascii,
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

fn default_displays() -> Vec<DisplayConfig> {
    vec![DisplayConfig::Ascii]
}
