use catan_core::gameplay::{
    game::{
        decision::DecisionKind,
        event::{EventTransaction, GameEvent, ObserverNotificationContext},
        output::GameOutput,
        projection::GameProjection,
        run::GameResult,
        view::VisibilityPolicy,
    },
    primitives::{
        build::{Build, EstablishmentType},
        dev_card::UsableDevCardSet,
        resource::ResourceSet,
    },
};
use catan_core::math::dice::DiceRoll;
use serde::{Deserialize, Serialize};
use statrs::distribution::{ChiSquared, ContinuousCDF};
use std::sync::LazyLock;

use crate::sync_host::{ObserverFrame, OutputObserver};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiceRollHistogram {
    pub counts: [u64; DiceRoll::COUNT],
}

impl Default for DiceRollHistogram {
    fn default() -> Self {
        Self {
            counts: [0; DiceRoll::COUNT],
        }
    }
}

impl DiceRollHistogram {
    pub fn record(&mut self, roll: DiceRoll) {
        self.counts[(roll.get() - DiceRoll::MIN_VALUE) as usize] += 1;
    }

    pub fn count(&self, roll: DiceRoll) -> u64 {
        self.counts[(roll.get() - DiceRoll::MIN_VALUE) as usize]
    }

    /// Calculates the p-value of the recorded sample using a Chi-Squared Goodness-of-Fit Test.
    /// Returns `None` if and only if the sample is completely empty.
    pub fn calculate_p_value(&self) -> Option<f64> {
        let total_rolls: u64 = self.counts.iter().sum();
        if total_rolls == 0 {
            return None;
        }
        let n = total_rolls as f64;

        let chi_squared_stat: f64 = self
            .counts
            .iter()
            .zip(&DiceRoll::ALL)
            .map(|(&observed_count, roll)| {
                let observed = observed_count as f64;
                let expected = n * (roll.prob_pts() as f64 / 36.0);

                (observed - expected).powi(2) / expected
            })
            .sum();

        static CHI_DIST: LazyLock<ChiSquared> =
            LazyLock::new(|| ChiSquared::new((DiceRoll::COUNT - 1) as f64).unwrap());

        Some(1.0 - CHI_DIST.cdf(chi_squared_stat))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFlowStats {
    pub distributed: ResourceSet,
    pub discarded: ResourceSet,
    pub stolen: ResourceSet,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildStats {
    pub roads: u64,
    pub settlements: u64,
    pub cities: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GameSummary {
    pub result: Option<GameResult>,
    pub run: GameRunStats,
    pub dice: DiceRollHistogram,
    pub resources: ResourceFlowStats,
    pub builds: BuildStats,
    pub dev_cards_used: UsableDevCardSet,
    pub final_view: Option<GameProjection>,
}

#[derive(Debug, Default, Clone)]
pub struct RunStatsCollector {
    stats: GameRunStats,
}

#[derive(Debug, Default, Clone)]
pub struct RunStatsObserver {
    summary: GameSummary,
    collector: RunStatsCollector,
}

impl RunStatsObserver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn summary(&self) -> &GameSummary {
        &self.summary
    }

    pub fn summary_owned(&self) -> GameSummary {
        self.summary.clone()
    }

    pub fn stats(&self) -> GameRunStats {
        self.summary.run
    }

    fn record_event(&mut self, event: &GameEvent, frame: &ObserverFrame<'_>) {
        self.collector.record_event(event);
        self.summary.run = self.collector.stats();

        match event {
            GameEvent::DiceRolled { value, .. } => self.summary.dice.record(*value),
            GameEvent::ResourcesDistributed { by_player } => {
                for (_, resources) in by_player {
                    self.summary.resources.distributed += *resources;
                }
            }
            GameEvent::PlayerDiscarded { resources, .. } => {
                self.summary.resources.discarded += *resources;
            }
            GameEvent::ResourceStolen { resource, .. } => {
                self.summary.resources.stolen[*resource] += 1;
            }
            GameEvent::Built { build, .. } => match build {
                Build::Road(_) => self.summary.builds.roads += 1,
                Build::Establishment(establishment) => match establishment.stage {
                    EstablishmentType::Settlement => self.summary.builds.settlements += 1,
                    EstablishmentType::City => self.summary.builds.cities += 1,
                },
            },
            GameEvent::DevCardUsed { usage, .. } => {
                self.summary.dev_cards_used[usage.card_kind()] += 1;
            }
            GameEvent::GameEnded { result } => {
                self.summary.result = Some(result.clone());
                self.summary.final_view = Some(GameProjection::from_observer(
                    ObserverNotificationContext::Omniscient {
                        public: frame.factory.public_view(VisibilityPolicy::Omniscient),
                        full: frame.factory.omniscient_view(),
                    },
                    false,
                ));
            }
            _ => {}
        }
    }
}

impl OutputObserver for RunStatsObserver {
    fn on_output(&mut self, frame: ObserverFrame<'_>) {
        let GameOutput::Event(record) = frame.output else {
            return;
        };
        self.record_event(&record.event, &frame);
    }
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
            GameEvent::GameEnded { result } => match result {
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
        tx.events.push(GameEvent::GameEnded {
            result: GameResult::Win(P0),
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

    #[test]
    fn stats_observer_records_dice_and_final_view() {
        use catan_core::dice_roll;
        use catan_core::gameplay::{
            game::{
                engine::GameEngine,
                event::{EventVisibility, GameEvent},
                output::{GameEventRecord, GameOutput},
                run::GameResult,
                state::SetupGameState,
                view::{ContextFactory, VisibilityConfig},
            },
            primitives::player::PlayerId,
        };

        let init = SetupGameState::default();
        let engine = GameEngine::from_init(init, Default::default());
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: engine.table(),
            index: engine.index(),
            visibility: &visibility,
            trade_sessions: engine.trade_sessions(),
        };
        let mut observer = RunStatsObserver::new();

        observer.on_output(ObserverFrame {
            output: &GameOutput::event(GameEventRecord {
                event: GameEvent::DiceRolled {
                    player_id: PlayerId::new(0),
                    value: dice_roll!(8),
                },
                visibility: EventVisibility::Public,
            }),
            factory: &factory,
            engine: &engine,
        });
        observer.on_output(ObserverFrame {
            output: &GameOutput::event(GameEventRecord {
                event: GameEvent::GameEnded {
                    result: GameResult::Win(PlayerId::new(0)),
                },
                visibility: EventVisibility::Public,
            }),
            factory: &factory,
            engine: &engine,
        });

        let summary = observer.summary();
        assert_eq!(summary.dice.count(dice_roll!(8)), 1);
        assert!(matches!(summary.result, Some(GameResult::Win(_))));
        assert!(summary.final_view.is_some());
        assert!(
            summary
                .final_view
                .as_ref()
                .expect("final view should be captured")
                .snapshot_state
                .is_none()
        );
    }
}
