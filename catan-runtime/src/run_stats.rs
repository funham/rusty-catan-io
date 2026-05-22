use catan_core::gameplay::game::{
    decision::DecisionKind,
    event::{EventTransaction, GameEvent},
    run::GameResult,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameRunStats {
    pub game_started: u64,
    pub turns_started: u64,
    pub turns_ended: u64,
    pub decision_requests: u64,
    pub regular_actions: u64,
    pub builds: u64,
    pub bank_trades: u64,
    pub dev_cards_bought: u64,
    pub dev_cards_used: u64,
    pub dice_rolls: u64,
    pub resources_distributed: u64,
    pub player_discards: u64,
    pub robber_moves: u64,
    pub action_rejections: u64,
    pub games_ended: u64,
    pub games_interrupted: u64,
}

#[derive(Debug, Default, Clone)]
pub struct RunStatsCollector {
    stats: GameRunStats,
}

impl RunStatsCollector {
    pub fn record_transaction(&mut self, transaction: &EventTransaction) {
        for event in &transaction.events {
            self.record_event(event);
        }
    }

    pub fn record_event(&mut self, event: &GameEvent) {
        match event {
            GameEvent::GameStarted => {
                self.stats.game_started += 1;
            }
            GameEvent::DecisionOpened(decision) => {
                if !matches!(decision.kind, DecisionKind::InitialPlacement) {
                    self.stats.decision_requests += 1;
                }
            }
            GameEvent::TurnStarted { .. } => {
                self.stats.turns_started += 1;
            }
            GameEvent::TurnEnded { .. } => {
                self.stats.turns_ended += 1;
                self.stats.regular_actions += 1;
            }
            GameEvent::Built { .. } => {
                self.stats.regular_actions += 1;
                self.stats.builds += 1;
            }
            GameEvent::DevCardBought { .. } => {
                self.stats.regular_actions += 1;
                self.stats.dev_cards_bought += 1;
            }
            GameEvent::DevCardUsed { .. } => {
                self.stats.dev_cards_used += 1;
            }
            GameEvent::BankTradeCompleted { .. } => {
                self.stats.regular_actions += 1;
                self.stats.bank_trades += 1;
            }
            GameEvent::DiceRolled { .. } => {
                self.stats.dice_rolls += 1;
            }
            GameEvent::ResourcesDistributed { .. } => {
                self.stats.resources_distributed += 1;
            }
            GameEvent::PlayerDiscarded { .. } => {
                self.stats.player_discards += 1;
            }
            GameEvent::RobberMoved { .. } => {
                self.stats.robber_moves += 1;
            }
            GameEvent::CommandRejected {
                counts_toward_limit: true,
                ..
            } => {
                self.stats.action_rejections += 1;
            }
            GameEvent::CommandRejected {
                counts_toward_limit: false,
                ..
            } => {}
            GameEvent::GameFinished { result, .. } => match result {
                GameResult::Win(_) => self.stats.games_ended += 1,
                GameResult::Interrupted { .. } | GameResult::LimitReached { .. } => {
                    self.stats.games_interrupted += 1;
                }
            },
            GameEvent::DecisionClosed { .. }
            | GameEvent::InitialPlacementBuilt { .. }
            | GameEvent::InitialResourcesGranted { .. }
            | GameEvent::DevCardDrawn { .. }
            | GameEvent::TradeOpened { .. }
            | GameEvent::TradeOfferAdded { .. }
            | GameEvent::TradeResponseUpdated { .. }
            | GameEvent::TradeCompleted { .. }
            | GameEvent::TradeCancelled { .. }
            | GameEvent::ResourceStolen { .. } => {}
        }
    }

    pub fn stats(&self) -> GameRunStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use catan_core::topology::{Hex, Path};
    use catan_core::{
        dice_roll,
        gameplay::{
            game::{
                decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision},
                event::{EventTransaction, GameEvent},
                input::DecisionToken,
                output::CommandRejectionReason,
                run::GameResult,
            },
            primitives::{
                build::{Build, Road},
                player::PlayerId,
                resource::ResourceSet,
            },
        },
    };

    use super::*;

    const P0: PlayerId = PlayerId::new(0);

    #[test]
    fn run_stats_collector_derives_old_reducer_counters_from_events() {
        let mut tx = EventTransaction::new(catan_core::gameplay::game::event::EventCause::Start);
        tx.events.push(GameEvent::GameStarted);
        tx.events.push(GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(1),
            player_id: P0,
            kind: DecisionKind::RegularCommand,
            lifetime: DecisionLifetime::OneShot,
        }));
        tx.events.push(GameEvent::TurnStarted {
            player_id: P0,
            turn_no: 0,
        });
        tx.events.push(GameEvent::Built {
            player_id: P0,
            build: Build::Road(Road {
                path: Path::try_from((Hex::new(0, 0), Hex::new(1, 0))).unwrap(),
            }),
        });
        tx.events.push(GameEvent::DevCardBought { player_id: P0 });
        tx.events.push(GameEvent::DiceRolled {
            player_id: P0,
            value: dice_roll!(8),
        });
        let mut by_player = catan_core::gameplay::game::event::ResourceDistribution::new();
        by_player.push((P0, ResourceSet::EMPTY));
        tx.events
            .push(GameEvent::ResourcesDistributed { by_player });
        tx.events.push(GameEvent::CommandRejected {
            token: DecisionToken {
                id: DecisionId(1),
                player_id: P0,
            },
            reason: CommandRejectionReason::WrongPhase,
            counts_toward_limit: true,
        });
        tx.events.push(GameEvent::TurnEnded {
            player_id: P0,
            turn_no: 0,
        });
        tx.events.push(GameEvent::GameFinished {
            result: GameResult::Win(P0),
            stats: None,
        });

        let mut collector = RunStatsCollector::default();
        collector.record_transaction(&tx);
        let stats = collector.stats();

        assert_eq!(stats.game_started, 1);
        assert_eq!(stats.decision_requests, 1);
        assert_eq!(stats.turns_started, 1);
        assert_eq!(stats.turns_ended, 1);
        assert_eq!(stats.regular_actions, 3);
        assert_eq!(stats.builds, 1);
        assert_eq!(stats.dev_cards_bought, 1);
        assert_eq!(stats.dice_rolls, 1);
        assert_eq!(stats.resources_distributed, 1);
        assert_eq!(stats.action_rejections, 1);
        assert_eq!(stats.games_ended, 1);
        assert_eq!(stats.games_interrupted, 0);
    }
}
