use std::{fs, path::Path};

use catan_runtime::config::{
    BotConfig, DiceConfig, FieldConfig, InitialStateConfig, LimitsConfig, LoggingConfig,
    PersistenceConfig,
};
use serde::{Deserialize, Deserializer, de};

#[derive(Debug, Deserialize)]
pub struct RemoteMatchConfig {
    pub players: Vec<SeatConfig>,
    #[serde(default)]
    pub observers: Vec<RemoteObserverConfig>,
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

#[derive(Debug, Clone)]
pub enum SeatConfig {
    Remote,
    Bot(BotConfig),
}

#[derive(Debug, Clone)]
pub enum RemoteObserverConfig {
    TuiSpectator,
    TuiPlayer { player_id: usize },
    TuiOmniscient,
    TuiSnapshot,
}

#[derive(Deserialize)]
struct TaggedConfig {
    kind: String,
    #[serde(default)]
    player_id: Option<usize>,
    #[serde(default)]
    bot: Option<BotConfig>,
}

impl<'de> Deserialize<'de> for SeatConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let tagged = TaggedConfig::deserialize(deserializer)?;
        match tagged.kind.as_str() {
            "remote" | "cli" => Ok(Self::Remote),
            "lazy" => Ok(Self::Bot(BotConfig::Lazy)),
            "greedy" => Ok(Self::Bot(BotConfig::Greedy)),
            "random" => Ok(Self::Bot(BotConfig::Random)),
            "bot" => tagged
                .bot
                .map(Self::Bot)
                .ok_or_else(|| de::Error::missing_field("bot")),
            other => Err(de::Error::unknown_variant(
                other,
                &["remote", "cli", "lazy", "greedy", "random", "bot"],
            )),
        }
    }
}

impl<'de> Deserialize<'de> for RemoteObserverConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let tagged = TaggedConfig::deserialize(deserializer)?;
        match tagged.kind.as_str() {
            "cli_spectator" | "tui_spectator" => Ok(Self::TuiSpectator),
            "cli_player" | "tui_player" => {
                let player_id = tagged
                    .player_id
                    .ok_or_else(|| de::Error::missing_field("player_id"))?;
                Ok(Self::TuiPlayer { player_id })
            }
            "cli_omniscient" | "tui_omniscient" => Ok(Self::TuiOmniscient),
            "snapshot_observer" | "tui_snapshot" => Ok(Self::TuiSnapshot),
            other => Err(de::Error::unknown_variant(
                other,
                &[
                    "cli_spectator",
                    "tui_spectator",
                    "cli_player",
                    "tui_player",
                    "cli_omniscient",
                    "tui_omniscient",
                    "snapshot_observer",
                    "tui_snapshot",
                ],
            )),
        }
    }
}

pub fn load_config(path: &Path) -> Result<RemoteMatchConfig, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {err}", path.display()))?;
    let mut config: RemoteMatchConfig = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {err}", path.display()))?;
    resolve_paths(&mut config, path.parent().unwrap_or_else(|| Path::new(".")));
    Ok(config)
}

pub fn resolve_paths(config: &mut RemoteMatchConfig, base: &Path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_remote_and_bot_seats() {
        let config: RemoteMatchConfig = serde_json::from_str(
            r#"{
              "players": [
                { "kind": "remote" },
                { "kind": "lazy" }
              ],
              "observers": [{ "kind": "tui_snapshot" }]
            }"#,
        )
        .unwrap();

        assert!(matches!(config.players[0], SeatConfig::Remote));
        assert!(matches!(
            config.players[1],
            SeatConfig::Bot(BotConfig::Lazy)
        ));
        assert!(matches!(
            config.observers[0],
            RemoteObserverConfig::TuiSnapshot
        ));
    }
}
