use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MatchConfig {
    pub players: Vec<BotConfig>,
    #[serde(default)]
    pub observers: Vec<ObserverConfig>,
    #[serde(default)]
    pub initial: InitialStateConfig,
    #[serde(default)]
    pub field: FieldConfig,
    #[serde(default)]
    pub dice: DiceConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub persistence: PersistenceConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BotConfig {
    Lazy,
    Greedy,
    Random,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObserverConfig {
    RunSummary,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InitialStateConfig {
    #[default]
    Fresh,
    Snapshot {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldConfig {
    #[default]
    Default,
    LayoutRef {
        path: PathBuf,
    },
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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistenceConfig {
    #[default]
    Off,
    JournalOnly {
        dir: PathBuf,
    },
    JournalWithCheckpoints {
        dir: PathBuf,
        checkpoint_every: u64,
    },
}

pub fn resolve_paths(config: &mut MatchConfig, base: &std::path::Path) {
    if let FieldConfig::LayoutRef { path } = &mut config.field
        && path.is_relative()
    {
        *path = base.join(&path);
    }
    if let InitialStateConfig::Snapshot { path } = &mut config.initial
        && path.is_relative()
    {
        *path = base.join(&path);
    }
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

#[cfg(test)]
mod tests {
    use super::{FieldConfig, InitialStateConfig, MatchConfig, PersistenceConfig};

    #[test]
    fn runtime_rejects_remote_player_config() {
        let err = serde_json::from_str::<MatchConfig>(r#"{ "players": [{ "kind": "remote" }] }"#)
            .unwrap_err();

        assert!(err.to_string().contains("unknown variant"));
    }

    #[test]
    fn runtime_parses_bot_players_directly() {
        let config: MatchConfig = serde_json::from_str(
            r#"{
              "players": [{ "kind": "lazy" }, { "kind": "greedy" }, { "kind": "random" }]
            }"#,
        )
        .unwrap();

        assert_eq!(config.players.len(), 3);
    }

    #[test]
    fn parses_layout_ref_field_config() {
        let config: MatchConfig = serde_json::from_str(
            r#"{
              "players": [{ "kind": "lazy" }],
              "field": { "kind": "layout_ref", "path": "../layouts/standard-4p.layout.json" }
            }"#,
        )
        .unwrap();

        assert!(matches!(config.field, FieldConfig::LayoutRef { .. }));
    }

    #[test]
    fn parses_snapshot_initial_state_config() {
        let config: MatchConfig = serde_json::from_str(
            r#"{
              "players": [{ "kind": "lazy" }],
              "initial": { "kind": "snapshot", "path": "target/snapshots/snapshot-000001" }
            }"#,
        )
        .unwrap();

        assert!(matches!(
            config.initial,
            InitialStateConfig::Snapshot { .. }
        ));
    }

    #[test]
    fn persistence_defaults_to_off_and_parses_checkpoint_mode() {
        let default_config: MatchConfig =
            serde_json::from_str(r#"{ "players": [{ "kind": "lazy" }] }"#).unwrap();
        assert!(matches!(default_config.persistence, PersistenceConfig::Off));

        let checkpoint_config: MatchConfig = serde_json::from_str(
            r#"{
              "players": [{ "kind": "lazy" }],
              "persistence": {
                "kind": "journal_with_checkpoints",
                "dir": "target/test-journal",
                "checkpoint_every": 25
              }
            }"#,
        )
        .unwrap();
        assert!(matches!(
            checkpoint_config.persistence,
            PersistenceConfig::JournalWithCheckpoints {
                checkpoint_every: 25,
                ..
            }
        ));
    }
}
