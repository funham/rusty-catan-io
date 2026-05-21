//! CLI command parsing and decision prompt handlers.
//!
//! Parses typed commands into core action types and coordinates higher-level interactive
//! flows by delegating visual choices to `CliUi` and legal-option lookups to selectors.

use std::io;

use catan_agents::{
    cli_command::{CliCommand, DevCardCommand, ParseCommandError, parse_cli_command},
    remote_agent::{DecisionRequestEnvelope, UiModel},
};
use catan_core::{
    constants,
    gameplay::game::command::{InitCommand, PostDiceCommand, RegularCommand},
    gameplay::game::{
        input::{TradeCommand, TradeResponseCommand},
        trade::{TradeOfferId, TradeScope, TradeSessionId},
    },
    gameplay::primitives::{
        build::{Build, EstablishmentType, Road},
        dev_card::{DevCardUsage, UsableDevCard},
        player::PlayerId,
        resource::ResourceSet,
        trade::{PersonalTradeOffer, PlayerTrade, PublicTradeOffer},
    },
    topology::{Hex, Intersection},
};

use super::{
    selectors::{
        knight_hexes, knight_rob_targets, legal_builds_for_mode, legal_initial_settlements,
        roadbuild_first_options, roadbuild_second_options,
    },
    tui::CliUi,
};

pub(crate) fn read_init_action(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
) -> io::Result<InitCommand> {
    log::trace!("Reading init action");
    loop {
        let model = &envelope.view;
        let line = ui.prompt(model, "command: ")?;
        match parse_cli_command(&line) {
            Ok(None) | Ok(Some(CliCommand::RollDice)) => {
                log::trace!("Init action: RollDice");
                return Ok(InitCommand::RollDice);
            }
            Ok(Some(CliCommand::DevCard(command))) => {
                match handle_dev_card_command(ui, envelope, command)? {
                    CommandOutcome::Accepted(usage) => {
                        log::trace!("Init action: UseDevCard({:?})", usage);
                        return Ok(InitCommand::UseDevCard(usage));
                    }
                    CommandOutcome::Handled => continue,
                    CommandOutcome::NotMatched => unreachable!("dev-card command should match"),
                }
            }
            Ok(Some(command)) => {
                set_invalid_use(
                    ui,
                    format!("{} is not available before rolling", command_label(command)),
                )?;
            }
            Err(err) => set_parse_error(ui, &line, err)?,
        }
    }
}

pub(crate) fn read_post_dice_action(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
) -> io::Result<PostDiceCommand> {
    log::trace!("Reading post-dice action");
    loop {
        let model = &envelope.view;
        let line = ui.prompt(model, "command: ")?;
        match parse_cli_command(&line) {
            Ok(None) => return Ok(PostDiceCommand::RegularCommand(RegularCommand::EndMove)),
            Ok(Some(CliCommand::RollDice)) => {
                set_invalid_use(ui, "dice have already been rolled".to_owned())?;
            }
            Ok(Some(CliCommand::DevCard(command))) => {
                match handle_dev_card_command(ui, envelope, command)? {
                    CommandOutcome::Accepted(usage) => {
                        log::trace!("Post-dice action: UseDevCard({:?})", usage);
                        return Ok(PostDiceCommand::UseDevCard(usage));
                    }
                    CommandOutcome::Handled => continue,
                    CommandOutcome::NotMatched => unreachable!("dev-card command should match"),
                }
            }
            Ok(Some(command)) => match handle_regular_command(ui, envelope, command)? {
                CommandOutcome::Accepted(action) => {
                    log::trace!("Post-dice action: RegularCommand({:?})", action);
                    return Ok(PostDiceCommand::RegularCommand(action));
                }
                CommandOutcome::Handled => continue,
                CommandOutcome::NotMatched => {
                    set_invalid_use(ui, "command is not a regular action".to_owned())?;
                }
            },
            Err(err) => set_parse_error(ui, &line, err)?,
        }
    }
}

pub(crate) fn read_regular_action(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
) -> io::Result<RegularCommand> {
    log::trace!("Reading regular action");
    loop {
        let model = &envelope.view;
        let line = ui.prompt(model, "command: ")?;
        match parse_cli_command(&line) {
            Ok(None) => return Ok(RegularCommand::EndMove),
            Ok(Some(CliCommand::RollDice)) => {
                set_invalid_use(ui, "dice have already been rolled".to_owned())?;
            }
            Ok(Some(CliCommand::DevCard(command))) => {
                match handle_dev_card_command(ui, envelope, command)? {
                    CommandOutcome::Accepted(usage) => {
                        log::trace!("Regular action: UseDevCard({:?})", usage);
                        return Ok(RegularCommand::UseDevCard(usage));
                    }
                    CommandOutcome::Handled => continue,
                    CommandOutcome::NotMatched => unreachable!("dev-card command should match"),
                }
            }
            Ok(Some(command)) => match handle_regular_command(ui, envelope, command)? {
                CommandOutcome::Accepted(action) => {
                    log::trace!("Regular action: {:?}", action);
                    return Ok(action);
                }
                CommandOutcome::Handled => continue,
                CommandOutcome::NotMatched => {
                    set_invalid_use(ui, "command is not a regular action".to_owned())?;
                }
            },
            Err(err) => set_parse_error(ui, &line, err)?,
        }
    }
}

enum CommandOutcome<T> {
    Accepted(T),
    Handled,
    NotMatched,
}

fn handle_regular_command(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    command: CliCommand,
) -> io::Result<CommandOutcome<RegularCommand>> {
    let model = &envelope.view;
    match command {
        CliCommand::Regular(action) => Ok(CommandOutcome::Accepted(action)),
        CliCommand::InteractiveBuildRoad => {
            select_interactive_build(ui, envelope, PartialBuildMode::Road)
        }
        CliCommand::InteractiveBuildSettlement => {
            select_interactive_build(ui, envelope, PartialBuildMode::Settlement)
        }
        CliCommand::InteractiveBuildCity => {
            select_interactive_build(ui, envelope, PartialBuildMode::City)
        }
        CliCommand::InteractiveBankTrade => {
            if envelope.legal.bank_trades.is_empty() {
                let reason = "no legal bank trades: missing resources or required port".to_owned();
                log::warn!(
                    target: "catan_runtime::cli_child::input",
                    "invalid command use: {reason}"
                );
                ui.set_message(format!("invalid command use: {reason}"))?;
                return Ok(CommandOutcome::Handled);
            }
            Ok(match ui.select_bank_trade(model, &envelope.legal)? {
                Some(trade) => CommandOutcome::Accepted(RegularCommand::TradeWithBank(trade)),
                None => CommandOutcome::Handled,
            })
        }
        CliCommand::InteractivePlayerTrade => select_interactive_player_trade(ui, envelope),
        CliCommand::PlayerTradeProposal { scope, offer } => {
            let action = match scope {
                TradeScope::Public => RegularCommand::OfferPublicTrade(PublicTradeOffer {
                    give: offer.give,
                    take: offer.take,
                }),
                TradeScope::Targeted(peer_id) => {
                    RegularCommand::OfferPersonalTrade(PersonalTradeOffer {
                        give: offer.give,
                        take: offer.take,
                        peer_id,
                    })
                }
            };
            Ok(CommandOutcome::Accepted(action))
        }
        CliCommand::RollDice | CliCommand::DevCard(_) => Ok(CommandOutcome::NotMatched),
    }
}

fn select_interactive_player_trade(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
) -> io::Result<CommandOutcome<RegularCommand>> {
    let Some(scope) = read_trade_scope(ui, &envelope.view)? else {
        return Ok(CommandOutcome::Handled);
    };
    let give = read_resource_collection(ui, &envelope.view, "trade give counts: ")?;
    let take = read_resource_collection(ui, &envelope.view, "trade take counts: ")?;
    let action = match scope {
        TradeScope::Public => RegularCommand::OfferPublicTrade(PublicTradeOffer { give, take }),
        TradeScope::Targeted(peer_id) => RegularCommand::OfferPersonalTrade(PersonalTradeOffer {
            give,
            take,
            peer_id,
        }),
    };
    Ok(CommandOutcome::Accepted(action))
}

fn read_trade_scope(ui: &mut CliUi, model: &UiModel) -> io::Result<Option<TradeScope>> {
    loop {
        let line = ui.prompt(model, "trade peer [public/pN/select]: ")?;
        match line.as_str() {
            "public" | "open" | "all" => return Ok(Some(TradeScope::Public)),
            "select" | "s" => {
                let candidates = trade_peer_candidates(model);
                return Ok(ui
                    .select_player(model, &candidates, "trade peer: ")?
                    .map(TradeScope::Targeted));
            }
            _ => {
                if let Some(raw) = line.strip_prefix('p')
                    && let Ok(peer_id) = raw.parse::<PlayerId>()
                {
                    return Ok(Some(TradeScope::Targeted(peer_id)));
                }
                ui.set_message("expected public, pN, or select".to_owned())?;
            }
        }
    }
}

fn trade_peer_candidates(model: &UiModel) -> Vec<PlayerId> {
    model
        .public
        .players
        .iter()
        .map(|player| player.player_id)
        .filter(|player_id| Some(*player_id) != model.actor)
        .collect()
}

pub(crate) fn read_trade_response_action(
    ui: &mut CliUi,
    model: &UiModel,
    session_id: TradeSessionId,
) -> io::Result<TradeResponseCommand> {
    loop {
        let line = ui.prompt(model, "trade [accept <id>|reject|counter]: ")?;
        let parts = line.split_whitespace().collect::<Vec<_>>();
        match parts.as_slice() {
            ["a" | "accept", offer_id] => match parse_offer_id(offer_id) {
                Some(offer_id) => return Ok(TradeResponseCommand::Accept { offer_id }),
                None => ui.set_message("expected unsigned offer id".to_owned())?,
            },
            ["r" | "reject" | "decline"] | [] => return Ok(TradeResponseCommand::Reject),
            ["c" | "counter"] => {
                let give = read_resource_collection(ui, model, "counter give counts: ")?;
                let take = read_resource_collection(ui, model, "counter take counts: ")?;
                return Ok(TradeResponseCommand::Counter {
                    offer: PlayerTrade { give, take },
                });
            }
            _ => ui.set_message(format!(
                "trade session {}: expected accept <id>, reject, or counter",
                session_id.0
            ))?,
        }
    }
}

pub(crate) fn read_trade_owner_action(
    ui: &mut CliUi,
    model: &UiModel,
    session_id: TradeSessionId,
) -> io::Result<TradeCommand> {
    loop {
        let line = ui.prompt(model, "trade owner [commit <id>|cancel]: ")?;
        let parts = line.split_whitespace().collect::<Vec<_>>();
        match parts.as_slice() {
            ["c" | "commit", offer_id] => match parse_offer_id(offer_id) {
                Some(offer_id) => return Ok(TradeCommand::Commit { offer_id }),
                None => ui.set_message("expected unsigned offer id".to_owned())?,
            },
            ["x" | "cancel"] | [] => return Ok(TradeCommand::Cancel),
            _ => ui.set_message(format!(
                "trade session {}: expected commit <id> or cancel",
                session_id.0
            ))?,
        }
    }
}

fn parse_offer_id(token: &str) -> Option<TradeOfferId> {
    token.parse::<u64>().ok().map(TradeOfferId)
}

fn select_interactive_build(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    kind: PartialBuildMode,
) -> io::Result<CommandOutcome<RegularCommand>> {
    let model = &envelope.view;
    let builds = legal_builds_for_mode(&envelope.legal, kind);
    if builds.is_empty() {
        let reason = build_unavailable_reason(model, kind);
        log::warn!(
            target: "catan_runtime::cli_child::input",
            "invalid command use: {reason}"
        );
        ui.set_message(format!("invalid command use: {reason}"))?;
        return Ok(CommandOutcome::Handled);
    }
    Ok(match ui.select_build(model, builds, "build: ")? {
        Some(build) => CommandOutcome::Accepted(RegularCommand::Build(build)),
        None => CommandOutcome::Handled,
    })
}

fn handle_dev_card_command(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    command: DevCardCommand,
) -> io::Result<CommandOutcome<DevCardUsage>> {
    match command {
        DevCardCommand::Interactive(card) => {
            handle_interactive_dev_card_action(ui, envelope, PartialDevCardMode::from_card(card))
        }
        DevCardCommand::Usage(usage) => {
            let mode = PartialDevCardMode::from_card(usage.card_kind());
            if !dev_card_usage_is_legal(envelope, usage) {
                let reason = dev_card_unavailable_reason(&envelope.view, envelope, mode);
                log::warn!(
                    target: "catan_runtime::cli_child::input",
                    "invalid command use: {reason}"
                );
                ui.set_message(format!("invalid command use: {reason}"))?;
                return Ok(CommandOutcome::Handled);
            }
            Ok(CommandOutcome::Accepted(usage))
        }
    }
}

fn handle_interactive_dev_card_action(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    mode: PartialDevCardMode,
) -> io::Result<CommandOutcome<DevCardUsage>> {
    let model = &envelope.view;
    if !dev_card_mode_has_legal_usage(envelope, mode) {
        let reason = dev_card_unavailable_reason(model, envelope, mode);
        log::warn!(
            target: "catan_runtime::cli_child::input",
            "invalid command use: {reason}"
        );
        ui.set_message(format!("invalid command use: {reason}"))?;
        return Ok(CommandOutcome::Handled);
    }

    let usage = match mode {
        PartialDevCardMode::Knight => select_knight_usage(ui, envelope)?,
        PartialDevCardMode::RoadBuild => select_roadbuild_usage(ui, envelope)?,
        PartialDevCardMode::Monopoly => ui
            .select_resource(
                model,
                "monopoly: ",
                "select monopoly resource with left/right",
            )?
            .map(DevCardUsage::Monopoly),
        PartialDevCardMode::YearOfPlenty => {
            let Some(first) = ui.select_resource(
                model,
                "year-of-plenty 1: ",
                "select first year-of-plenty resource",
            )?
            else {
                return Ok(CommandOutcome::Handled);
            };
            let Some(second) = ui.select_resource(
                model,
                "year-of-plenty 2: ",
                "select second year-of-plenty resource",
            )?
            else {
                return Ok(CommandOutcome::Handled);
            };
            Some(DevCardUsage::YearOfPlenty([first, second]))
        }
    };

    Ok(match usage {
        Some(usage) => CommandOutcome::Accepted(usage),
        None => CommandOutcome::Handled,
    })
}

fn dev_card_mode_has_legal_usage(
    envelope: &DecisionRequestEnvelope,
    mode: PartialDevCardMode,
) -> bool {
    if envelope.legal.dev_card_used_this_turn {
        return false;
    }
    envelope
        .legal
        .dev_card_usages
        .iter()
        .any(|usage| usage.card_kind() == mode.card_kind())
}

fn dev_card_usage_is_legal(envelope: &DecisionRequestEnvelope, usage: DevCardUsage) -> bool {
    if envelope.legal.dev_card_used_this_turn {
        return false;
    }
    envelope.legal.dev_card_usages.contains(&usage)
}

fn dev_card_unavailable_reason(
    model: &UiModel,
    envelope: &DecisionRequestEnvelope,
    mode: PartialDevCardMode,
) -> String {
    if envelope.legal.dev_card_used_this_turn {
        return "development card already used this turn".to_owned();
    }

    let Some(private) = &model.private else {
        return format!(
            "no legal {} usage and private card data is unavailable",
            mode.label()
        );
    };

    let card = mode.card_kind();
    if private.dev_cards.active[card] == 0 && private.dev_cards.queued[card] > 0 {
        return format!("{} is queued until next turn", mode.label());
    }

    if private.dev_cards.active[card] == 0 {
        return format!("no active {} card is available", mode.label());
    }

    format!("no legal {} usage is available now", mode.label())
}

fn select_knight_usage(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
) -> io::Result<Option<DevCardUsage>> {
    let model = &envelope.view;
    let legal_hexes = knight_hexes(&envelope.legal);
    let rob_hex = ui.select_hex_where(model, "knight hex: ", |hex| legal_hexes.contains(&hex))?;
    let candidates = knight_rob_targets(&envelope.legal, rob_hex);
    let robbed_id = ui.select_player(model, &candidates, "robbed player: ")?;
    if robbed_id.is_none() && !candidates.is_empty() {
        return Ok(None);
    }
    Ok(Some(DevCardUsage::Knight { rob_hex, robbed_id }))
}

fn select_roadbuild_usage(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
) -> io::Result<Option<DevCardUsage>> {
    let model = &envelope.view;
    let first_options = roadbuild_first_options(&envelope.legal);
    let Some(Build::Road(first)) = ui.select_build(model, first_options, "roadbuild 1: ")? else {
        return Ok(None);
    };

    let second_options = roadbuild_second_options(&envelope.legal, first.path);
    let Some(Build::Road(second)) = ui.select_build(model, second_options, "roadbuild 2: ")? else {
        return Ok(None);
    };

    Ok(Some(DevCardUsage::RoadBuild([first.path, second.path])))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartialDevCardMode {
    Knight,
    RoadBuild,
    Monopoly,
    YearOfPlenty,
}

impl PartialDevCardMode {
    fn from_card(card: UsableDevCard) -> Self {
        match card {
            UsableDevCard::Knight => Self::Knight,
            UsableDevCard::YearOfPlenty => Self::YearOfPlenty,
            UsableDevCard::RoadBuild => Self::RoadBuild,
            UsableDevCard::Monopoly => Self::Monopoly,
        }
    }

    fn card_kind(self) -> UsableDevCard {
        match self {
            Self::Knight => UsableDevCard::Knight,
            Self::RoadBuild => UsableDevCard::RoadBuild,
            Self::Monopoly => UsableDevCard::Monopoly,
            Self::YearOfPlenty => UsableDevCard::YearOfPlenty,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Knight => "knight",
            Self::RoadBuild => "road building",
            Self::Monopoly => "monopoly",
            Self::YearOfPlenty => "year of plenty",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartialBuildMode {
    Settlement,
    Road,
    City,
}

impl PartialBuildMode {
    fn label(self) -> &'static str {
        match self {
            Self::Settlement => "settlement",
            Self::Road => "road",
            Self::City => "city",
        }
    }

    fn piece_limit(self) -> usize {
        match self {
            Self::Settlement => 5,
            Self::Road => 15,
            Self::City => 5,
        }
    }

    fn cost(self) -> ResourceSet {
        match self {
            Self::Settlement => constants::costs::SETTLEMENT,
            Self::Road => constants::costs::ROAD,
            Self::City => constants::costs::CITY,
        }
    }
}

fn build_unavailable_reason(model: &UiModel, kind: PartialBuildMode) -> String {
    let Some(actor) = model.actor else {
        return format!("no legal {} placements: no active player", kind.label());
    };

    let placed = player_piece_count(model, actor, kind);
    if placed >= kind.piece_limit() {
        return format!(
            "no legal {} placements: p{} already has the maximum {} {}s",
            kind.label(),
            actor,
            kind.piece_limit(),
            kind.label()
        );
    }

    let Some(private) = &model.private else {
        return format!(
            "no legal {} placements: private resource data is unavailable",
            kind.label()
        );
    };

    let cost = kind.cost();
    if !private.resources.has_enough(&cost) {
        return format!(
            "no legal {} placements: p{} cannot afford cost {} with {}",
            kind.label(),
            actor,
            cost,
            private.resources
        );
    }

    format!(
        "no legal {} placements: no connected legal board positions are available",
        kind.label()
    )
}

fn player_piece_count(model: &UiModel, actor: PlayerId, kind: PartialBuildMode) -> usize {
    let Some(builds) = model
        .public
        .builds
        .iter()
        .find(|builds| builds.player_id == actor)
    else {
        return 0;
    };

    match kind {
        PartialBuildMode::Settlement => builds
            .establishments
            .iter()
            .filter(|establishment| establishment.stage == EstablishmentType::Settlement)
            .count(),
        PartialBuildMode::City => builds
            .establishments
            .iter()
            .filter(|establishment| establishment.stage == EstablishmentType::City)
            .count(),
        PartialBuildMode::Road => builds.roads.len(),
    }
}

fn set_parse_error(ui: &mut CliUi, line: &str, err: ParseCommandError) -> io::Result<()> {
    let message = format!("command syntax error: {err}");
    log::warn!(
        target: "catan_runtime::cli_child::input",
        "{message}; input={line:?}"
    );
    ui.set_message(message)
}

fn set_invalid_use(ui: &mut CliUi, cause: String) -> io::Result<()> {
    let message = format!("invalid command use: {cause}");
    log::warn!(target: "catan_runtime::cli_child::input", "{message}");
    ui.set_message(message)
}

fn command_label(command: CliCommand) -> &'static str {
    match command {
        CliCommand::RollDice => "roll",
        CliCommand::Regular(RegularCommand::EndMove) => "end",
        CliCommand::Regular(RegularCommand::BuyDevCard) => "buy-dev",
        CliCommand::Regular(RegularCommand::Build(_))
        | CliCommand::InteractiveBuildRoad
        | CliCommand::InteractiveBuildSettlement
        | CliCommand::InteractiveBuildCity => "build",
        CliCommand::Regular(RegularCommand::TradeWithBank(_))
        | CliCommand::InteractiveBankTrade => "bank-trade",
        CliCommand::InteractivePlayerTrade
        | CliCommand::PlayerTradeProposal { .. }
        | CliCommand::Regular(RegularCommand::OfferPublicTrade(_))
        | CliCommand::Regular(RegularCommand::OfferPersonalTrade(_)) => "player-trade",
        CliCommand::Regular(RegularCommand::UseDevCard(_)) | CliCommand::DevCard(_) => "dev-card",
    }
}

pub(crate) fn read_resource_collection(
    ui: &mut CliUi,
    model: &UiModel,
    prompt: &str,
) -> io::Result<ResourceSet> {
    log::trace!("Reading resource collection");
    loop {
        let line = ui.prompt(model, prompt)?;
        if line == "drop" {
            if let Some(resources) = ui.select_drop_cards(model)? {
                return Ok(resources);
            }
            continue;
        }
        let parts = line
            .split_whitespace()
            .map(str::parse::<u16>)
            .collect::<Result<Vec<_>, _>>();
        match parts {
            Ok(parts) if parts.len() == 5 => {
                let resources = ResourceSet {
                    brick: parts[0],
                    wood: parts[1],
                    wheat: parts[2],
                    sheep: parts[3],
                    ore: parts[4],
                };
                log::trace!("Resource collection read: {:?}", resources);
                return Ok(resources);
            }
            _ => {
                log::warn!("Invalid resource collection input: {}", line);
                ui.set_message("expected five unsigned integers".to_owned())?
            }
        }
    }
}

pub(crate) fn read_initial_settlement(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    prompt: &str,
) -> io::Result<Intersection> {
    let legal = legal_initial_settlements(&envelope.legal);
    ui.select_intersection_where(&envelope.view, prompt, |intersection| {
        legal.contains(&intersection)
    })
}

pub(crate) fn read_initial_road(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    settlement: Intersection,
    prompt: &str,
) -> io::Result<Road> {
    ui.select_initial_road(&envelope.view, &envelope.legal, settlement, prompt)
}

pub(crate) fn read_hex(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    prompt: &str,
) -> io::Result<Hex> {
    log::trace!(
        target: "catan_runtime::cli_child::input",
        "reading robber hex from {} legal hexes",
        envelope.legal.robber_hexes.len()
    );
    ui.select_hex_where(&envelope.view, prompt, |hex| {
        envelope.legal.robber_hexes.contains(&hex)
    })
}

fn read_player_id(ui: &mut CliUi, model: &UiModel, prompt: &str) -> io::Result<PlayerId> {
    log::trace!("Reading player ID");
    loop {
        let line = ui.prompt(model, prompt)?;
        if let Ok(id) = line.parse() {
            log::trace!("Player ID read: {}", id);
            return Ok(id);
        }
        log::warn!("Invalid player ID input: {}", line);
        ui.set_message("expected unsigned integer".to_owned())?;
    }
}

pub(crate) fn read_robbed_player(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    prompt: &str,
) -> io::Result<PlayerId> {
    if let Some(player_id) =
        ui.select_player(&envelope.view, &envelope.legal.rob_targets, prompt)?
    {
        return Ok(player_id);
    }

    ui.set_message("rob target selection cancelled".to_owned())?;
    read_player_id(ui, &envelope.view, prompt)
}
