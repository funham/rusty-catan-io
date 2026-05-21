use std::{error::Error, fmt};

use catan_core::{
    gameplay::{
        game::{command::RegularCommand, trade::TradeScope},
        primitives::{
            build::{Build, Establishment, EstablishmentType, Road},
            dev_card::{DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::{Resource, ResourceSet},
            trade::{BankTrade, BankTradeKind, PlayerTrade},
        },
    },
    topology::{HexIndex, Intersection, Path, repr::Dual},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    RollDice,
    Regular(RegularCommand),
    DevCard(DevCardCommand),
    InteractiveBuildRoad,
    InteractiveBuildSettlement,
    InteractiveBuildCity,
    InteractiveBankTrade,
    InteractivePlayerTrade,
    PlayerTradeProposal {
        scope: TradeScope,
        offer: PlayerTrade,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevCardCommand {
    Interactive(UsableDevCard),
    Usage(DevCardUsage),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCommandError {
    kind: ParseCommandErrorKind,
    cause: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseCommandErrorKind {
    UnknownCommand,
    InvalidArguments,
}

impl ParseCommandError {
    pub fn kind(&self) -> ParseCommandErrorKind {
        self.kind
    }

    pub fn cause(&self) -> &str {
        &self.cause
    }

    fn unknown(command: &str) -> Self {
        Self {
            kind: ParseCommandErrorKind::UnknownCommand,
            cause: format!("unknown command '{command}'"),
        }
    }

    fn invalid(cause: impl Into<String>) -> Self {
        Self {
            kind: ParseCommandErrorKind::InvalidArguments,
            cause: cause.into(),
        }
    }
}

impl fmt::Display for ParseCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.cause)
    }
}

impl Error for ParseCommandError {}

pub fn parse_cli_command(line: &str) -> Result<Option<CliCommand>, ParseCommandError> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    if matches!(line, "roll" | "r") {
        return Ok(Some(CliCommand::RollDice));
    }
    if matches!(line, "end" | "e") {
        return Ok(Some(CliCommand::Regular(RegularCommand::EndMove)));
    }
    if matches!(line, "buy dev" | "buy-dev" | "bd") {
        return Ok(Some(CliCommand::Regular(RegularCommand::BuyDevCard)));
    }

    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["br"] | ["build", "road"] => Ok(Some(CliCommand::InteractiveBuildRoad)),
        ["bs"] | ["build", "settlement"] => Ok(Some(CliCommand::InteractiveBuildSettlement)),
        ["bc"] | ["build", "city"] => Ok(Some(CliCommand::InteractiveBuildCity)),
        ["bt"] | ["bank-trade"] => Ok(Some(CliCommand::InteractiveBankTrade)),
        ["pt"] | ["player-trade"] => Ok(Some(CliCommand::InteractivePlayerTrade)),
        ["trade", scope, give, take] => Ok(Some(CliCommand::PlayerTradeProposal {
            scope: parse_trade_scope(scope)?,
            offer: PlayerTrade {
                give: parse_resource(give)?.into(),
                take: parse_resource(take)?.into(),
            },
        })),
        ["trade", scope, b1, w1, wh1, s1, o1, b2, w2, wh2, s2, o2] => {
            Ok(Some(CliCommand::PlayerTradeProposal {
                scope: parse_trade_scope(scope)?,
                offer: PlayerTrade {
                    give: parse_resource_set_counts([b1, w1, wh1, s1, o1])?,
                    take: parse_resource_set_counts([b2, w2, wh2, s2, o2])?,
                },
            }))
        }
        ["kn"] | ["use", "knight"] => Ok(Some(interactive_dev(UsableDevCard::Knight))),
        ["yp"] | ["use", "yop"] | ["use", "year-of-plenty"] => {
            Ok(Some(interactive_dev(UsableDevCard::YearOfPlenty)))
        }
        ["m"] | ["use", "monopoly"] => Ok(Some(interactive_dev(UsableDevCard::Monopoly))),
        ["rb"] | ["use", "roadbuild"] | ["use", "road-build"] => {
            Ok(Some(interactive_dev(UsableDevCard::RoadBuild)))
        }
        ["build", "road", h1, h2] => Ok(Some(CliCommand::Regular(RegularCommand::Build(
            Build::Road(Road {
                path: path_from_tokens(h1, h2)?,
            }),
        )))),
        ["build", "settlement", h1, h2, h3] => Ok(Some(CliCommand::Regular(
            RegularCommand::Build(Build::Establishment(Establishment {
                vtx: intersection_from_tokens(h1, h2, h3)?,
                stage: EstablishmentType::Settlement,
            })),
        ))),
        ["build", "city", h1, h2, h3] => Ok(Some(CliCommand::Regular(RegularCommand::Build(
            Build::Establishment(Establishment {
                vtx: intersection_from_tokens(h1, h2, h3)?,
                stage: EstablishmentType::City,
            }),
        )))),
        ["bank-trade", give, take, kind] => Ok(Some(CliCommand::Regular(
            RegularCommand::TradeWithBank(BankTrade {
                give: parse_resource(give)?,
                take: parse_resource(take)?,
                kind: parse_bank_trade_kind(kind)?,
            }),
        ))),
        ["use", "knight", hex] => Ok(Some(dev_usage(DevCardUsage::Knight {
            rob_hex: hex_from_token(hex, "invalid knight hex")?,
            robbed_id: None,
        }))),
        ["use", "knight", hex, "none"] => Ok(Some(dev_usage(DevCardUsage::Knight {
            rob_hex: hex_from_token(hex, "invalid knight hex")?,
            robbed_id: None,
        }))),
        ["use", "knight", hex, robbed_id] => Ok(Some(dev_usage(DevCardUsage::Knight {
            rob_hex: hex_from_token(hex, "invalid knight hex")?,
            robbed_id: Some(
                robbed_id
                    .parse()
                    .map_err(|_| ParseCommandError::invalid("invalid robbed player id"))?,
            ),
        }))),
        ["use", "yop", first, second] | ["use", "year-of-plenty", first, second] => {
            Ok(Some(dev_usage(DevCardUsage::YearOfPlenty([
                parse_resource(first)?,
                parse_resource(second)?,
            ]))))
        }
        ["use", "monopoly", resource] => Ok(Some(dev_usage(DevCardUsage::Monopoly(
            parse_resource(resource)?,
        )))),
        ["use", "roadbuild", h1, h2, h3, h4] | ["use", "road-build", h1, h2, h3, h4] => {
            Ok(Some(dev_usage(DevCardUsage::RoadBuild([
                path_from_tokens(h1, h2)?,
                path_from_tokens(h3, h4)?,
            ]))))
        }
        [command, ..] => Err(ParseCommandError::unknown(command)),
        [] => Ok(None),
    }
}

fn interactive_dev(card: UsableDevCard) -> CliCommand {
    CliCommand::DevCard(DevCardCommand::Interactive(card))
}

fn dev_usage(usage: DevCardUsage) -> CliCommand {
    CliCommand::DevCard(DevCardCommand::Usage(usage))
}

fn parse_resource(token: &str) -> Result<Resource, ParseCommandError> {
    match token.to_ascii_lowercase().as_str() {
        "brick" => Ok(Resource::Brick),
        "wood" => Ok(Resource::Wood),
        "wheat" => Ok(Resource::Wheat),
        "sheep" => Ok(Resource::Sheep),
        "ore" => Ok(Resource::Ore),
        _ => Err(ParseCommandError::invalid(format!(
            "unknown resource '{token}'"
        ))),
    }
}

fn parse_resource_set_counts(tokens: [&str; 5]) -> Result<ResourceSet, ParseCommandError> {
    let [brick, wood, wheat, sheep, ore] = tokens
        .map(|token| token.parse::<u16>())
        .map(|value| value.map_err(|_| ParseCommandError::invalid("invalid resource count")));
    Ok(ResourceSet {
        brick: brick?,
        wood: wood?,
        wheat: wheat?,
        sheep: sheep?,
        ore: ore?,
    })
}

fn parse_trade_scope(token: &str) -> Result<TradeScope, ParseCommandError> {
    if matches!(token, "public" | "open" | "all") {
        return Ok(TradeScope::Public);
    }
    if let Some(raw) = token.strip_prefix('p') {
        let peer_id = raw
            .parse::<PlayerId>()
            .map_err(|_| ParseCommandError::invalid("invalid trade player id"))?;
        return Ok(TradeScope::Targeted(peer_id));
    }
    Err(ParseCommandError::invalid(format!(
        "unknown trade scope '{token}'"
    )))
}

fn parse_bank_trade_kind(token: &str) -> Result<BankTradeKind, ParseCommandError> {
    match token {
        "G4" | "common" => Ok(BankTradeKind::BankGeneric),
        "G3" | "port-3" => Ok(BankTradeKind::PortGeneric),
        "S2" | "port-2" => Ok(BankTradeKind::PortSpecific),
        _ => Err(ParseCommandError::invalid(format!(
            "unknown bank trade kind '{token}'"
        ))),
    }
}

fn hex_from_token(
    token: &str,
    cause: &'static str,
) -> Result<catan_core::topology::Hex, ParseCommandError> {
    let index = token
        .parse()
        .map_err(|_| ParseCommandError::invalid(cause))?;
    Ok(HexIndex::spiral_to_hex(index))
}

fn path_from_tokens(h1: &str, h2: &str) -> Result<Path, ParseCommandError> {
    let h1 = hex_from_token(h1, "invalid path hex")?;
    let h2 = hex_from_token(h2, "invalid path hex")?;
    Path::try_from((h1, h2))
        .or_else(|_| Path::<Dual>::try_from((h1, h2)).map(|path| path.canon()))
        .map_err(|_| ParseCommandError::invalid("expected adjacent hex pair"))
}

fn intersection_from_tokens(
    h1: &str,
    h2: &str,
    h3: &str,
) -> Result<Intersection, ParseCommandError> {
    Intersection::try_from([
        hex_from_token(h1, "invalid intersection hex")?,
        hex_from_token(h2, "invalid intersection hex")?,
        hex_from_token(h3, "invalid intersection hex")?,
    ])
    .map_err(|_| ParseCommandError::invalid("expected adjacent hex triplet"))
}
