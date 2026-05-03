//! CLI command parsing and decision prompt handlers.
//!
//! Parses typed commands into core action types and coordinates higher-level interactive
//! flows by delegating visual choices to `CliUi` and legal-option lookups to selectors.

use std::io;

use catan_agents::remote_agent::{DecisionRequestEnvelope, UiModel};
use catan_core::{
    agent::action::{InitAction, PostDiceAction, RegularAction},
    constants,
    gameplay::primitives::{
        build::{Build, Establishment, EstablishmentType, Road},
        dev_card::DevCardUsage,
        player::PlayerId,
        resource::{Resource, ResourceCollection},
        trade::{BankTrade, BankTradeKind},
    },
    topology::{Hex, HexIndex, Intersection, Path as BoardPath, repr::Dual},
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
) -> io::Result<InitAction> {
    log::trace!("Reading init action");
    loop {
        let model = &envelope.view;
        let line = ui.prompt(model, "action [roll]: ")?;
        let line = line.trim();
        if line.is_empty() || matches!(line, "roll" | "r") {
            log::trace!("Init action: RollDice");
            return Ok(InitAction::RollDice);
        }
        if let Some(usage) = handle_interactive_dev_card_action(ui, envelope, line)? {
            log::trace!("Init interactive action: UseDevCard({:?})", usage);
            return Ok(InitAction::UseDevCard(usage));
        }
        if partial_dev_card_command(line).is_some() {
            continue;
        }
        if let Some(usage) = parse_dev_card_usage(line) {
            log::trace!("Init action: UseDevCard({:?})", usage);
            return Ok(InitAction::UseDevCard(usage));
        }
        log::warn!("Could not parse init action: {}", line);
        ui.set_message("could not parse action".to_owned())?;
    }
}

pub(crate) fn read_post_dice_action(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
) -> io::Result<PostDiceAction> {
    log::trace!("Reading post-dice action");
    loop {
        let model = &envelope.view;
        let line = ui.prompt(model, "action: ")?;
        if let Some(usage) = parse_dev_card_usage(&line) {
            log::trace!("Post-dice action: UseDevCard({:?})", usage);
            return Ok(PostDiceAction::UseDevCard(usage));
        }
        if let Some(usage) = handle_interactive_dev_card_action(ui, envelope, &line)? {
            log::trace!("Post-dice interactive action: UseDevCard({:?})", usage);
            return Ok(PostDiceAction::UseDevCard(usage));
        }
        if partial_dev_card_command(&line).is_some() {
            continue;
        }
        match handle_interactive_regular_action(ui, envelope, &line)? {
            CommandOutcome::Accepted(action) => {
                log::trace!("Post-dice interactive action: RegularAction({:?})", action);
                return Ok(PostDiceAction::RegularAction(action));
            }
            CommandOutcome::Handled => continue,
            CommandOutcome::NotMatched => {}
        }
        if let Some(action) = parse_regular_action(&line) {
            log::trace!("Post-dice action: RegularAction({:?})", action);
            return Ok(PostDiceAction::RegularAction(action));
        }
        log::warn!("Could not parse post-dice action: {}", line);
        ui.set_message("could not parse action".to_owned())?;
    }
}

pub(crate) fn read_regular_action(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
) -> io::Result<RegularAction> {
    log::trace!("Reading regular action");
    loop {
        let model = &envelope.view;
        let line = ui.prompt(model, "action: ")?;
        match handle_interactive_regular_action(ui, envelope, &line)? {
            CommandOutcome::Accepted(action) => {
                log::trace!("Regular interactive action: {:?}", action);
                return Ok(action);
            }
            CommandOutcome::Handled => continue,
            CommandOutcome::NotMatched => {}
        }
        if let Some(action) = parse_regular_action(&line) {
            log::trace!("Regular action: {:?}", action);
            return Ok(action);
        }
        log::warn!("Could not parse regular action: {}", line);
        ui.set_message("could not parse action".to_owned())?;
    }
}

enum CommandOutcome<T> {
    Accepted(T),
    Handled,
    NotMatched,
}

fn handle_interactive_regular_action(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    line: &str,
) -> io::Result<CommandOutcome<RegularAction>> {
    let model = &envelope.view;
    if let Some(kind) = partial_build_command(line) {
        let builds = legal_builds_for_mode(&envelope.legal, kind);
        if builds.is_empty() {
            let reason = build_unavailable_reason(model, kind);
            log::warn!(
                target: "catan_runtime::cli_child::input",
                "Build command recognized but no legal {} placements are available: {reason}",
                kind.label()
            );
            ui.set_message(reason)?;
            return Ok(CommandOutcome::Handled);
        }
        return Ok(match ui.select_build(model, builds, "build: ")? {
            Some(build) => CommandOutcome::Accepted(RegularAction::Build(build)),
            None => CommandOutcome::Handled,
        });
    }
    if matches!(line, "bank-trade" | "bt") {
        if envelope.legal.bank_trades.is_empty() {
            let reason = "no legal bank trades: missing resources or required port".to_owned();
            log::warn!(
                target: "catan_runtime::cli_child::input",
                "Bank trade command recognized but {reason}"
            );
            ui.set_message(reason)?;
            return Ok(CommandOutcome::Handled);
        }
        return Ok(match ui.select_bank_trade(model, &envelope.legal)? {
            Some(trade) => CommandOutcome::Accepted(RegularAction::TradeWithBank(trade)),
            None => CommandOutcome::Handled,
        });
    }
    Ok(CommandOutcome::NotMatched)
}

#[cfg(test)]
fn partial_regular_command(line: &str) -> bool {
    partial_build_command(line).is_some() || matches!(line, "bank-trade" | "bt")
}

fn handle_interactive_dev_card_action(
    ui: &mut CliUi,
    envelope: &DecisionRequestEnvelope,
    line: &str,
) -> io::Result<Option<DevCardUsage>> {
    let model = &envelope.view;
    match partial_dev_card_command(line) {
        Some(PartialDevCardMode::Knight) => select_knight_usage(ui, envelope),
        Some(PartialDevCardMode::RoadBuild) => select_roadbuild_usage(ui, envelope),
        Some(PartialDevCardMode::Monopoly) => Ok(ui
            .select_resource(
                model,
                "monopoly: ",
                "select monopoly resource with left/right",
            )?
            .map(DevCardUsage::Monopoly)),
        Some(PartialDevCardMode::YearOfPlenty) => {
            let Some(first) = ui.select_resource(
                model,
                "year-of-plenty 1: ",
                "select first year-of-plenty resource",
            )?
            else {
                return Ok(None);
            };
            let Some(second) = ui.select_resource(
                model,
                "year-of-plenty 2: ",
                "select second year-of-plenty resource",
            )?
            else {
                return Ok(None);
            };
            Ok(Some(DevCardUsage::YearOfPlenty([first, second])))
        }
        None => Ok(None),
    }
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

    let second_options = roadbuild_second_options(&envelope.legal, first.pos);
    let Some(Build::Road(second)) = ui.select_build(model, second_options, "roadbuild 2: ")? else {
        return Ok(None);
    };

    Ok(Some(DevCardUsage::RoadBuild([first.pos, second.pos])))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartialDevCardMode {
    Knight,
    RoadBuild,
    Monopoly,
    YearOfPlenty,
}

pub(crate) fn partial_dev_card_command(line: &str) -> Option<PartialDevCardMode> {
    match line.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["use", "knight"] | ["kn"] => Some(PartialDevCardMode::Knight),
        ["use", "roadbuild"] | ["use", "road-build"] | ["rb"] => {
            Some(PartialDevCardMode::RoadBuild)
        }
        ["use", "monopoly"] | ["m"] => Some(PartialDevCardMode::Monopoly),
        ["use", "yop"] | ["use", "year-of-plenty"] | ["yp"] => {
            Some(PartialDevCardMode::YearOfPlenty)
        }
        _ => None,
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
            Self::City => 4,
        }
    }

    fn cost(self) -> ResourceCollection {
        match self {
            Self::Settlement => constants::costs::SETTLEMENT,
            Self::Road => constants::costs::ROAD,
            Self::City => constants::costs::CITY,
        }
    }
}

pub(crate) fn partial_build_command(line: &str) -> Option<PartialBuildMode> {
    match line.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["build", "settlement"] | ["bs"] => Some(PartialBuildMode::Settlement),
        ["build", "road"] | ["br"] => Some(PartialBuildMode::Road),
        ["build", "city"] | ["bc"] => Some(PartialBuildMode::City),
        _ => None,
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

pub(crate) fn parse_regular_action(line: &str) -> Option<RegularAction> {
    let line = line.trim();
    if matches!(line, "end" | "e") || line.is_empty() {
        return Some(RegularAction::EndMove);
    }
    if matches!(line, "buy dev" | "buy-dev" | "bd") {
        return Some(RegularAction::BuyDevCard);
    }
    if let Some(build) = parse_build(line) {
        return Some(RegularAction::Build(build));
    }
    if let Some(trade) = parse_bank_trade(line) {
        return Some(RegularAction::TradeWithBank(trade));
    }
    None
}

fn parse_build(line: &str) -> Option<Build> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["build", "road", h1, h2] => Some(Build::Road(Road {
            pos: path_from_tokens(h1, h2)?,
        })),
        ["build", "settlement", h1, h2, h3] => Some(Build::Establishment(Establishment {
            pos: intersection_from_tokens(h1, h2, h3)?,
            stage: EstablishmentType::Settlement,
        })),
        ["build", "city", h1, h2, h3] => Some(Build::Establishment(Establishment {
            pos: intersection_from_tokens(h1, h2, h3)?,
            stage: EstablishmentType::City,
        })),
        _ => None,
    }
}

fn parse_bank_trade(line: &str) -> Option<BankTrade> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["bank-trade", give, take, kind] => Some(BankTrade {
            give: parse_resource(give)?,
            take: parse_resource(take)?,
            kind: match *kind {
                "G4" => BankTradeKind::BankGeneric,
                "G3" => BankTradeKind::PortGeneric,
                "S2" => BankTradeKind::PortSpecific,
                _ => return None,
            },
        }),
        _ => None,
    }
}

fn parse_dev_card_usage(line: &str) -> Option<DevCardUsage> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["use", "knight", hex] => Some(DevCardUsage::Knight {
            rob_hex: HexIndex::spiral_to_hex(hex.parse().ok()?),
            robbed_id: None,
        }),
        ["use", "knight", hex, "none"] => Some(DevCardUsage::Knight {
            rob_hex: HexIndex::spiral_to_hex(hex.parse().ok()?),
            robbed_id: None,
        }),
        ["use", "knight", hex, robbed_id] => Some(DevCardUsage::Knight {
            rob_hex: HexIndex::spiral_to_hex(hex.parse().ok()?),
            robbed_id: Some(robbed_id.parse().ok()?),
        }),
        ["use", "yop", first, second] | ["use", "year-of-plenty", first, second] => {
            Some(DevCardUsage::YearOfPlenty([
                parse_resource(first)?,
                parse_resource(second)?,
            ]))
        }
        ["use", "monopoly", resource] => Some(DevCardUsage::Monopoly(parse_resource(resource)?)),
        ["use", "roadbuild", h1, h2, h3, h4] | ["use", "road-build", h1, h2, h3, h4] => {
            Some(DevCardUsage::RoadBuild([
                path_from_tokens(h1, h2)?,
                path_from_tokens(h3, h4)?,
            ]))
        }
        _ => None,
    }
}

fn parse_resource(token: &str) -> Option<Resource> {
    match token.to_lowercase().as_str() {
        "brick" => Some(Resource::Brick),
        "wood" => Some(Resource::Wood),
        "wheat" => Some(Resource::Wheat),
        "sheep" => Some(Resource::Sheep),
        "ore" => Some(Resource::Ore),
        _ => None,
    }
}

fn path_from_tokens(h1: &str, h2: &str) -> Option<BoardPath> {
    let h1 = HexIndex::spiral_to_hex(h1.parse().ok()?);
    let h2 = HexIndex::spiral_to_hex(h2.parse().ok()?);
    BoardPath::try_from((h1, h2))
        .or_else(|_| BoardPath::<Dual>::try_from((h1, h2)).map(|path| path.canon()))
        .ok()
}

fn intersection_from_tokens(h1: &str, h2: &str, h3: &str) -> Option<Intersection> {
    Intersection::try_from([
        HexIndex::spiral_to_hex(h1.parse().ok()?),
        HexIndex::spiral_to_hex(h2.parse().ok()?),
        HexIndex::spiral_to_hex(h3.parse().ok()?),
    ])
    .ok()
}

pub(crate) fn read_resource_collection(
    ui: &mut CliUi,
    model: &UiModel,
    prompt: &str,
) -> io::Result<ResourceCollection> {
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
                let resources = ResourceCollection {
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

#[cfg(test)]
mod tests {
    use catan_core::agent::action::RegularAction;

    use super::{
        PartialBuildMode, PartialDevCardMode, parse_regular_action, partial_build_command,
        partial_dev_card_command, partial_regular_command,
    };

    #[test]
    fn partial_build_commands_parse_aliases() {
        assert_eq!(
            partial_build_command("build settlement"),
            Some(PartialBuildMode::Settlement)
        );
        assert_eq!(
            partial_build_command("bs"),
            Some(PartialBuildMode::Settlement)
        );
        assert_eq!(
            partial_build_command("build road"),
            Some(PartialBuildMode::Road)
        );
        assert_eq!(partial_build_command("br"), Some(PartialBuildMode::Road));
        assert_eq!(
            partial_build_command("build city"),
            Some(PartialBuildMode::City)
        );
        assert_eq!(partial_build_command("bc"), Some(PartialBuildMode::City));
        assert_eq!(partial_build_command("build road 0 1"), None);
    }

    #[test]
    fn interactive_shortcuts_parse() {
        assert!(partial_regular_command("bt"));
        assert!(matches!(
            parse_regular_action("bd"),
            Some(RegularAction::BuyDevCard)
        ));
        assert!(matches!(
            parse_regular_action("e"),
            Some(RegularAction::EndMove)
        ));
        assert_eq!(
            partial_dev_card_command("kn"),
            Some(PartialDevCardMode::Knight)
        );
        assert_eq!(
            partial_dev_card_command("use knight"),
            Some(PartialDevCardMode::Knight)
        );
        assert_eq!(
            partial_dev_card_command("rb"),
            Some(PartialDevCardMode::RoadBuild)
        );
        assert_eq!(
            partial_dev_card_command("use roadbuild"),
            Some(PartialDevCardMode::RoadBuild)
        );
        assert_eq!(
            partial_dev_card_command("m"),
            Some(PartialDevCardMode::Monopoly)
        );
        assert_eq!(
            partial_dev_card_command("yp"),
            Some(PartialDevCardMode::YearOfPlenty)
        );
    }
}
