use std::{
    io::{self, BufRead, Write},
    sync::{Arc, Mutex},
};

use catan_core::{
    gameplay::{
        game::{
            command::{
                ChooseRobbedPlayerCommand, DropHalfCommand, InitCommand, InitialPlacementCommand,
                MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand,
            },
            decision::DecisionKind,
            event::{GameEvent, PlayerNotification},
            input::DecisionRequest,
            input::{PlayerCommand, TradeCommand, TradeResponseCommand},
            trade::{TradeOfferId, TradeSessionId},
            view::{PlayerDecisionContext, PlayerNotificationContext},
        },
        primitives::{player::PlayerId, resource::ResourceSet},
    },
    topology::{Hex, HexIndex, Intersection, Path, repr::Dual},
};

use crate::{
    bot::BotPolicy,
    cli_command::{CliCommand, parse_cli_command},
};

#[derive(Debug, Default)]
struct TerminalUi;

#[derive(Clone, Debug, Default)]
pub struct SharedTerminalUi {
    inner: Arc<Mutex<TerminalUi>>,
}

#[derive(Clone, Debug)]
pub struct CliAgent {
    player_id: PlayerId,
    terminal: SharedTerminalUi,
}

impl CliAgent {
    pub fn new(id: impl Into<PlayerId>, terminal: SharedTerminalUi) -> Self {
        let id = id.into();
        Self {
            player_id: id,
            terminal,
        }
    }
}

impl PlayerNotification for CliAgent {
    fn on_event(&mut self, event: &GameEvent, context: PlayerNotificationContext<'_>) {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_notification(event, &context);
    }
}

impl CliAgent {
    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitialPlacementCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Initial placement", &context);
        loop {
            if let Some(action) = InitialPlacementCommand::try_new(
                TerminalUi::read_intersection("settlement (h1 h2 h3): "),
                TerminalUi::read_path("road (h1 h2): "),
            ) {
                break action;
            }

            log::warn!("incorrect initial stage placement")
        }
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Before dice", &context);
        InitCommand::RollDice
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("After dice", &context);
        PostDiceCommand::RegularCommand(TerminalUi::read_regular_action())
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardCommand {
        PostDevCardCommand::RollDice
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Action", &context);
        TerminalUi::read_regular_action()
    }

    fn move_robber(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobberCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Move robber", &context);
        MoveRobberCommand(TerminalUi::read_hex("robber hex: "))
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        _robber_pos: Hex,
    ) -> ChooseRobbedPlayerCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Choose player to rob", &context);
        ChooseRobbedPlayerCommand(TerminalUi::read_player_id("player id: "))
    }

    fn answer_trade(
        &mut self,
        context: PlayerDecisionContext<'_>,
        session: TradeSessionId,
    ) -> TradeResponseCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Trade response", &context);
        TerminalUi::read_trade_response(session)
    }

    fn trade_owner_action(
        &mut self,
        context: PlayerDecisionContext<'_>,
        session: TradeSessionId,
    ) -> TradeCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Trade owner action", &context);
        TerminalUi::read_trade_owner_action(session)
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Discard half", &context);
        DropHalfCommand(TerminalUi::read_resource_collection(
            "drop brick wood wheat sheep ore: ",
        ))
    }
}

impl BotPolicy for CliAgent {
    fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn command_for(
        &mut self,
        request: &DecisionRequest,
        context: PlayerDecisionContext<'_>,
    ) -> Option<PlayerCommand> {
        match request.kind() {
            DecisionKind::InitialPlacement => Some(PlayerCommand::InitialPlacement(
                self.init_stage_action(context),
            )),
            DecisionKind::InitCommand => {
                Some(PlayerCommand::InitCommand(self.init_action(context)))
            }
            DecisionKind::PostDiceCommand => {
                Some(PlayerCommand::PostDice(self.after_dice_action(context)))
            }
            DecisionKind::PostDevCardCommand => Some(PlayerCommand::PostDevCard(
                self.after_dev_card_action(context),
            )),
            DecisionKind::RegularCommand => {
                Some(PlayerCommand::Regular(self.regular_action(context)))
            }
            DecisionKind::MoveRobber => Some(PlayerCommand::MoveRobber(self.move_robber(context))),
            DecisionKind::ChooseRobbedPlayer { robber_pos } => Some(
                PlayerCommand::ChooseRobbedPlayer(self.choose_player_to_rob(context, robber_pos)),
            ),
            DecisionKind::DropHalf { .. } => Some(PlayerCommand::DropHalf(self.drop_half(context))),
            DecisionKind::TradeResponse { session } => Some(PlayerCommand::Trade(
                TradeCommand::Respond(self.answer_trade(context, session)),
            )),
            DecisionKind::TradeOwnerAction { session } => Some(PlayerCommand::Trade(
                self.trade_owner_action(context, session),
            )),
        }
    }
}

impl TerminalUi {
    fn print_notification(event: &GameEvent, _context: &PlayerNotificationContext<'_>) {
        println!("event: {event:?}");
    }

    fn print_decision_context(label: &str, context: &PlayerDecisionContext<'_>) {
        let mut renderer = crate::cli_agent::ui::field_render::FieldRenderer::new();
        renderer.draw_context(&context.public);
        renderer.render();

        println!("\n== {label} ==");
        println!("player: {}", context.actor);
        println!("resources: {}", context.private.resources);
        println!("robber: {:?}", context.public.board_state.robber_pos);
        for player in &context.public.players {
            println!("player {} => {:?}", player.player_id, player.resources);
        }
    }

    pub fn parse_regular_action(line: &str) -> Option<RegularCommand> {
        match parse_cli_command(line).ok().flatten() {
            None => Some(RegularCommand::EndMove),
            Some(CliCommand::Regular(action)) => Some(action),
            Some(CliCommand::PlayerTradeProposal { scope, offer }) => match scope {
                catan_core::gameplay::game::trade::TradeScope::Public => {
                    Some(RegularCommand::OfferPublicTrade(
                        catan_core::gameplay::primitives::trade::PublicTradeOffer {
                            give: offer.give,
                            take: offer.take,
                        },
                    ))
                }
                catan_core::gameplay::game::trade::TradeScope::Targeted(peer_id) => {
                    Some(RegularCommand::OfferPersonalTrade(
                        catan_core::gameplay::primitives::trade::PersonalTradeOffer {
                            give: offer.give,
                            take: offer.take,
                            peer_id,
                        },
                    ))
                }
            },
            _ => None,
        }
    }

    fn read_regular_action() -> RegularCommand {
        loop {
            let line = Self::read_line(
                "command [end | buy dev | build road ... | build settlement ... | build city ... | bank-trade give take kind]: ",
            );
            if let Some(action) = Self::parse_regular_action(&line) {
                return action;
            }
            println!("could not parse action");
        }
    }

    fn read_resource_collection(prompt: &str) -> ResourceSet {
        loop {
            let line = Self::read_line(prompt);
            let parts = line
                .split_whitespace()
                .map(str::parse::<u16>)
                .collect::<Result<Vec<_>, _>>();
            match parts {
                Ok(parts) if parts.len() == 5 => {
                    return ResourceSet {
                        brick: parts[0],
                        wood: parts[1],
                        wheat: parts[2],
                        sheep: parts[3],
                        ore: parts[4],
                    };
                }
                _ => println!("expected five unsigned integers"),
            }
        }
    }

    fn read_hex(prompt: &str) -> Hex {
        loop {
            let line = Self::read_line(prompt);
            if let Ok(index) = line.parse::<usize>() {
                return HexIndex::spiral_to_hex(index);
            }
            println!("expected spiral hex 0-based index");
        }
    }

    fn read_path(prompt: &str) -> Path {
        loop {
            let line = Self::read_line(prompt);
            let parts = line
                .split_whitespace()
                .map(str::parse::<usize>)
                .collect::<Result<Vec<_>, _>>();
            if let Ok(parts) = parts
                && parts.len() == 2
            {
                let h1 = HexIndex::spiral_to_hex(parts[0]);
                let h2 = HexIndex::spiral_to_hex(parts[1]);
                if let Ok(path) = Path::try_from((h1, h2)) {
                    return path;
                }
                if let Ok(path) = Path::<Dual>::try_from((h1, h2)) {
                    return path.canon();
                }
            }
            println!("expected adjacent hex pair");
        }
    }

    fn read_intersection(prompt: &str) -> Intersection {
        loop {
            let line = Self::read_line(prompt);
            let parts = line
                .split_whitespace()
                .map(str::parse::<usize>)
                .collect::<Result<Vec<_>, _>>();
            if let Ok(parts) = parts
                && parts.len() == 3
                && let Ok(intersection) = Intersection::try_from([
                    HexIndex::spiral_to_hex(parts[0]),
                    HexIndex::spiral_to_hex(parts[1]),
                    HexIndex::spiral_to_hex(parts[2]),
                ])
            {
                return intersection;
            }
            println!("expected adjacent hex triplet");
        }
    }

    fn read_player_id(prompt: &str) -> PlayerId {
        loop {
            let line = Self::read_line(prompt);
            if let Ok(id) = line.parse() {
                return id;
            }
            println!("expected unsigned integer");
        }
    }

    fn read_trade_response(session: TradeSessionId) -> TradeResponseCommand {
        loop {
            let line = Self::read_line(&format!(
                "trade session {} [accept <offer_id> | reject | counter <5 give counts> <5 take counts>]: ",
                session.0
            ));
            let parts = line.split_whitespace().collect::<Vec<_>>();
            match parts.as_slice() {
                ["accept", offer_id] | ["a", offer_id] => match offer_id.parse::<u64>() {
                    Ok(offer_id) => {
                        return TradeResponseCommand::Accept {
                            offer_id: TradeOfferId(offer_id),
                        };
                    }
                    Err(_) => println!("expected unsigned offer id"),
                },
                ["reject"] | ["r"] | ["decline"] => return TradeResponseCommand::Reject,
                ["counter", b1, w1, wh1, s1, o1, b2, w2, wh2, s2, o2] => {
                    if let Some(offer) = Self::resource_sets_from_counts(
                        [b1, w1, wh1, s1, o1],
                        [b2, w2, wh2, s2, o2],
                    ) {
                        return TradeResponseCommand::Counter {
                            offer: catan_core::gameplay::primitives::trade::PlayerTrade {
                                give: offer.0,
                                take: offer.1,
                            },
                        };
                    }
                    println!("expected ten unsigned resource counts");
                }
                _ => println!("expected accept, reject, or counter"),
            }
        }
    }

    fn read_trade_owner_action(session: TradeSessionId) -> TradeCommand {
        loop {
            let line = Self::read_line(&format!(
                "trade session {} [commit <offer_id> | cancel]: ",
                session.0
            ));
            let parts = line.split_whitespace().collect::<Vec<_>>();
            match parts.as_slice() {
                ["commit", offer_id] | ["c", offer_id] => match offer_id.parse::<u64>() {
                    Ok(offer_id) => {
                        return TradeCommand::Commit {
                            offer_id: TradeOfferId(offer_id),
                        };
                    }
                    Err(_) => println!("expected unsigned offer id"),
                },
                ["cancel"] | ["x"] => return TradeCommand::Cancel,
                _ => println!("expected commit or cancel"),
            }
        }
    }

    fn resource_sets_from_counts(
        give: [&str; 5],
        take: [&str; 5],
    ) -> Option<(ResourceSet, ResourceSet)> {
        Some((
            ResourceSet {
                brick: give[0].parse().ok()?,
                wood: give[1].parse().ok()?,
                wheat: give[2].parse().ok()?,
                sheep: give[3].parse().ok()?,
                ore: give[4].parse().ok()?,
            },
            ResourceSet {
                brick: take[0].parse().ok()?,
                wood: take[1].parse().ok()?,
                wheat: take[2].parse().ok()?,
                sheep: take[3].parse().ok()?,
                ore: take[4].parse().ok()?,
            },
        ))
    }

    fn read_line(prompt: &str) -> String {
        print!("{prompt}");
        io::stdout().flush().expect("failed to flush stdout");

        let mut line = String::new();
        io::stdin()
            .lock()
            .read_line(&mut line)
            .expect("failed to read line");
        line.trim().to_owned()
    }
}
