//! User-facing event journal formatting for the CLI.

use std::collections::VecDeque;

use catan_core::gameplay::game::projection::GameProjection;
use catan_core::gameplay::{
    game::{event::GameEvent, run::GameResult, trade::TradeResponseState},
    primitives::{
        PlayerId,
        board::Tile,
        build::{Build, EstablishmentType},
        dev_card::DevCardUsage,
        resource::{Resource, ResourceSet},
        trade::{BankTrade, BankTradeKind, PlayerTrade},
    },
};

#[derive(Debug, Clone)]
pub struct EventJournal {
    entries: VecDeque<JournalEntry>,
    capacity: usize,
    last_player_id: Option<PlayerId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalEntry {
    Event {
        text: String,
        player_id: Option<PlayerId>,
    },
    Divider,
}

impl JournalEntry {
    #[allow(dead_code)]
    pub fn is_divider(&self) -> bool {
        matches!(self, Self::Divider)
    }

    #[allow(dead_code)]
    pub fn is_event(&self) -> bool {
        matches!(self, Self::Event { .. })
    }

    #[allow(dead_code)]
    pub fn player_id(&self) -> Option<PlayerId> {
        match self {
            JournalEntry::Event { player_id, .. } => *player_id,
            JournalEntry::Divider => None,
        }
    }
}

impl EventJournal {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
            last_player_id: None,
        }
    }

    #[cfg(test)]
    pub fn push_event(&mut self, event: &GameEvent) -> Option<String> {
        self.push_event_with_model(event, None)
    }

    pub fn push_event_with_model(
        &mut self,
        event: &GameEvent,
        model: Option<&GameProjection>,
    ) -> Option<String> {
        let lines = meaningful_event_entries(event, model);
        let first = lines.first().map(|(line, _)| line.clone())?;

        for (idx, (line, player_id)) in lines.into_iter().enumerate() {
            let needs_divider = idx == 0
                && matches!(
                    (self.last_player_id, player_id),
                    (Some(last_id), Some(curr_id)) if curr_id != last_id
                );

            if needs_divider {
                self.push_entry(JournalEntry::Divider);
            }

            self.push_entry(JournalEntry::Event {
                text: line,
                player_id,
            });

            self.last_player_id = player_id;
        }

        Some(first)
    }

    fn push_entry(&mut self, entry: JournalEntry) {
        if self.capacity > 0 {
            self.entries.push_back(entry);
            self.trim_to_capacity();
        }
    }

    fn trim_to_capacity(&mut self) {
        while self
            .entries
            .iter()
            .filter(|entry| !entry.is_divider())
            .count()
            > self.capacity
        {
            self.entries.pop_front();
        }
        while self.entries.front().is_some_and(|entry| entry.is_divider()) {
            self.entries.pop_front();
        }
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &JournalEntry> {
        self.entries.iter()
    }
}

#[cfg(test)]
pub fn meaningful_event_line(event: &GameEvent) -> Option<String> {
    meaningful_event_line_with_model(event, None)
}

#[cfg(test)]
pub fn meaningful_event_line_with_model(
    event: &GameEvent,
    model: Option<&GameProjection>,
) -> Option<String> {
    meaningful_event_entries(event, model)
        .into_iter()
        .next()
        .map(|(line, _)| line)
}

fn meaningful_event_entries(
    event: &GameEvent,
    model: Option<&GameProjection>,
) -> Vec<(String, Option<PlayerId>)> {
    let line = match event {
        GameEvent::GameStarted => "game started".to_owned(),
        GameEvent::TurnStarted { player_id, turn_no } => {
            format!("turn {turn_no}: p{player_id} to play")
        }
        GameEvent::TurnEnded { player_id, .. } => format!("p{player_id} ended the turn"),
        GameEvent::InitialPlacementBuilt {
            player_id,
            settlement,
            road,
        } => format!(
            "p{player_id} placed initial settlement {} and road {}",
            intersection_label(*settlement),
            path_label(road.path)
        ),
        GameEvent::InitialResourcesGranted {
            player_id,
            resources,
        } => format!(
            "p{player_id} received initial resources {}",
            resource_set_mini_marker_label(resources)
        ),
        GameEvent::DiceRolled { player_id, value } => {
            format!("p{player_id} rolled {}", value.get())
        }
        GameEvent::ResourcesDistributed { by_player } => {
            let grants = by_player
                .iter()
                .filter(|(_, resources)| !resources.is_empty())
                .map(|(player_id, resources)| {
                    (
                        format!(
                            "p{player_id} got {}",
                            resource_set_mini_marker_label(resources)
                        ),
                        Some(*player_id),
                    )
                })
                .collect::<Vec<_>>();
            return if grants.is_empty() {
                vec![("no resources were produced".to_owned(), None)]
            } else {
                grants
            };
        }
        GameEvent::DevCardBought { player_id } => {
            format!("p{player_id} bought a development card")
        }
        GameEvent::DevCardUsed { player_id, usage } => {
            format!("p{player_id} used {}", dev_card_usage_label(usage))
        }
        GameEvent::Built { player_id, build } => {
            format!("p{player_id} built {}", build_label(*build))
        }
        GameEvent::BankTradeCompleted { player_id, trade } => {
            format!(
                "p{player_id} traded with bank: {}",
                bank_trade_label(*trade)
            )
        }
        GameEvent::TradeOpened {
            proposer_id, offer, ..
        } => format!(
            "p{proposer_id} opened public trade: {}",
            player_trade_label(offer)
        ),
        GameEvent::TradeOfferAdded {
            player_id, offer, ..
        } => format!("p{player_id} offered trade: {}", player_trade_label(offer)),
        GameEvent::TradeResponseUpdated {
            player_id,
            response,
            ..
        } => format!(
            "p{player_id} {} a trade",
            match response {
                TradeResponseState::Waiting => "is considering",
                TradeResponseState::Accepted { .. } => "accepted",
                TradeResponseState::Rejected => "declined",
                TradeResponseState::Countered { .. } => "countered",
            }
        ),
        GameEvent::TradeCompleted {
            proposer_id,
            peer_id,
            ..
        } => format!("p{proposer_id} traded with p{peer_id}"),
        GameEvent::TradeCancelled { proposer_id, .. } => {
            format!("p{proposer_id} cancelled a trade")
        }
        GameEvent::PlayerDiscarded {
            player_id,
            resources,
        } => format!(
            "p{player_id} discarded {}",
            resource_set_mini_marker_label(resources)
        ),
        GameEvent::RobberMoved {
            player_id,
            hex,
            robbed_id,
        } => match robbed_id {
            Some(_) => format!(
                "p{player_id} moved the robber to {}",
                robber_hex_label(*hex, model)
            ),
            None => format!(
                "p{player_id} moved the robber to {}",
                robber_hex_label(*hex, model)
            ),
        },
        GameEvent::ResourceStolen {
            player_id,
            robbed_id,
            resource,
        } => format!(
            "p{player_id} stole {} from p{robbed_id}",
            stolen_resource_label(*player_id, *robbed_id, *resource, model)
        ),
        GameEvent::GameEnded { result } => match result {
            GameResult::Win(player_id) => format!("p{player_id} won the game"),
            GameResult::Interrupted { reason } => format!("game interrupted: {reason}"),
            GameResult::LimitReached { turns } => format!("turn limit reached after {turns} turns"),
        },
        GameEvent::DecisionOpened(_)
        | GameEvent::DecisionClosed { .. }
        | GameEvent::CommandRejected { .. }
        | GameEvent::DevCardDrawn { .. } => return Vec::new(),
    };

    vec![(line, event_player_id(event))]
}

fn event_player_id(event: &GameEvent) -> Option<PlayerId> {
    match event {
        GameEvent::TurnStarted { player_id, .. }
        | GameEvent::TurnEnded { player_id, .. }
        | GameEvent::InitialPlacementBuilt { player_id, .. }
        | GameEvent::InitialResourcesGranted { player_id, .. }
        | GameEvent::DiceRolled { player_id, .. }
        | GameEvent::DevCardBought { player_id }
        | GameEvent::DevCardUsed { player_id, .. }
        | GameEvent::Built { player_id, .. }
        | GameEvent::BankTradeCompleted { player_id, .. }
        | GameEvent::TradeOfferAdded { player_id, .. }
        | GameEvent::TradeResponseUpdated { player_id, .. }
        | GameEvent::PlayerDiscarded { player_id, .. }
        | GameEvent::RobberMoved { player_id, .. }
        | GameEvent::ResourceStolen { player_id, .. } => Some(*player_id),
        GameEvent::TradeOpened { proposer_id, .. }
        | GameEvent::TradeCompleted { proposer_id, .. }
        | GameEvent::TradeCancelled { proposer_id, .. } => Some(*proposer_id),
        GameEvent::GameEnded { result } => match result {
            GameResult::Win(player_id) => Some(*player_id),
            GameResult::Interrupted { .. } | GameResult::LimitReached { .. } => None,
        },
        GameEvent::GameStarted
        | GameEvent::ResourcesDistributed { .. }
        | GameEvent::DecisionOpened(_)
        | GameEvent::DecisionClosed { .. }
        | GameEvent::CommandRejected { .. }
        | GameEvent::DevCardDrawn { .. } => None,
    }
}

fn build_label(build: Build) -> String {
    match build {
        Build::Road(road) => format!("road {}", path_label(road.path)),
        Build::Establishment(establishment) => match establishment.stage {
            EstablishmentType::Settlement => {
                format!("settlement {}", intersection_label(establishment.vtx))
            }
            EstablishmentType::City => format!("city {}", intersection_label(establishment.vtx)),
        },
    }
}

fn dev_card_usage_label(usage: &DevCardUsage) -> String {
    match usage {
        DevCardUsage::Knight { robbed_id, .. } => match robbed_id {
            Some(player_id) => format!("Knight against p{player_id}"),
            None => "Knight".to_owned(),
        },
        DevCardUsage::YearOfPlenty([first, second]) => {
            format!("Year of Plenty for {first:?} and {second:?}")
        }
        DevCardUsage::RoadBuild(_) => "Road Building".to_owned(),
        DevCardUsage::Monopoly(resource) => format!("Monopoly for {resource:?}"),
    }
}

fn bank_trade_label(trade: BankTrade) -> String {
    let rate = match trade.kind {
        BankTradeKind::BankGeneric => 4,
        BankTradeKind::PortGeneric => 3,
        BankTradeKind::PortSpecific => 2,
    };
    format!(
        "[{rate}{}] -> [1{}]",
        resource_marker_code(trade.give),
        resource_marker_code(trade.take)
    )
}

fn player_trade_label(trade: &PlayerTrade) -> String {
    format!(
        "give {} for {}",
        resource_set_mini_marker_label(&trade.give),
        resource_set_mini_marker_label(&trade.take)
    )
}

fn resource_set_mini_marker_label(resources: &ResourceSet) -> String {
    let parts = Resource::iter()
        .filter_map(|resource| {
            let count = resources[resource];
            (count > 0).then(|| format!("[{count}{}]", resource_marker_code(resource)))
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "nothing".to_owned()
    } else {
        parts.join("")
    }
}

fn resource_marker_code(resource: Resource) -> &'static str {
    match resource {
        Resource::Brick => "B",
        Resource::Wood => "W",
        Resource::Wheat => "H",
        Resource::Sheep => "S",
        Resource::Ore => "O",
    }
}

fn robber_hex_label(hex: catan_core::topology::Hex, model: Option<&GameProjection>) -> String {
    let Some(tile) = model.and_then(|model| model.public.board.tiles.get(hex.index().to_spiral()))
    else {
        return format!("h{}", hex.index().to_spiral());
    };
    match tile {
        Tile::Resource { resource, number } => format!("{resource:?} {} hex", number.get()),
        Tile::River { number } => format!("River {} hex", number.get()),
        Tile::Desert => "Desert hex".to_owned(),
    }
}

fn stolen_resource_label(
    player_id: PlayerId,
    robbed_id: PlayerId,
    resource: Resource,
    model: Option<&GameProjection>,
) -> String {
    match model_player_perspective(model) {
        Some(viewer) if viewer == player_id || viewer == robbed_id => {
            format!("[1{}]", resource_marker_code(resource))
        }
        Some(_) | None => "[?]".to_owned(),
    }
}

fn model_player_perspective(model: Option<&GameProjection>) -> Option<PlayerId> {
    model.and_then(|model| {
        model
            .private
            .as_ref()
            .map(|private| private.player_id)
            .or(model.actor)
    })
}

fn intersection_label(intersection: catan_core::topology::Intersection) -> String {
    let mut hexes = intersection
        .as_set()
        .into_iter()
        .map(|hex| hex.index().to_spiral())
        .collect::<Vec<_>>();
    hexes.sort_unstable();
    format!("v{:?}", hexes)
}

fn path_label(path: catan_core::topology::Path) -> String {
    let (a, b) = path.as_pair();
    let mut hexes = vec![a.index().to_spiral(), b.index().to_spiral()];
    hexes.sort_unstable();
    format!("e{:?}", hexes)
}

#[cfg(test)]
mod tests {
    use catan_core::{
        dice_roll,
        gameplay::{
            game::{
                decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision},
                event::GameEvent,
            },
            primitives::{dev_card::UsableDevCard, player::PlayerId},
        },
    };

    use crate::journal::JournalEntry;

    use super::{EventJournal, meaningful_event_line};

    const P0: PlayerId = PlayerId::new(0);
    const P1: PlayerId = PlayerId::new(1);

    #[test]
    fn journal_formats_meaningful_player_events() {
        let dice = GameEvent::DiceRolled {
            player_id: P1,
            value: dice_roll!(7),
        };
        let bought = GameEvent::DevCardBought { player_id: P0 };
        let used = GameEvent::DevCardUsed {
            player_id: P1,
            usage: catan_core::gameplay::primitives::dev_card::DevCardUsage::Monopoly(
                catan_core::gameplay::primitives::resource::Resource::Ore,
            ),
        };

        assert_eq!(meaningful_event_line(&dice), Some("p1 rolled 7".to_owned()));
        assert_eq!(
            meaningful_event_line(&bought),
            Some("p0 bought a development card".to_owned())
        );
        assert_eq!(
            meaningful_event_line(&used),
            Some("p1 used Monopoly for Ore".to_owned())
        );
    }

    #[test]
    fn journal_filters_technical_decision_events() {
        let technical = GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(4),
            player_id: P0,
            kind: DecisionKind::RegularCommand,
            lifetime: DecisionLifetime::OneShot,
        });

        assert_eq!(meaningful_event_line(&technical), None);
    }

    #[test]
    fn journal_splits_resource_distribution_by_player_with_mini_card_markers() {
        let mut by_player = catan_core::gameplay::game::event::ResourceDistribution::new();
        by_player.push((
            P0,
            catan_core::gameplay::primitives::resource::ResourceSet {
                brick: 4,
                ..Default::default()
            },
        ));
        by_player.push((
            P1,
            catan_core::gameplay::primitives::resource::ResourceSet {
                wood: 1,
                sheep: 2,
                ..Default::default()
            },
        ));
        let mut journal = EventJournal::new(8);

        journal.push_event(&GameEvent::ResourcesDistributed { by_player });

        let entries = journal
            .entries()
            .filter_map(|entry| match entry {
                JournalEntry::Event { text, player_id } => Some((text.as_str(), *player_id)),
                JournalEntry::Divider => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec![("p0 got [4B]", Some(P0)), ("p1 got [1W][2S]", Some(P1)),]
        );
    }

    #[test]
    fn journal_uses_mini_card_markers_for_all_quantified_resources() {
        use catan_core::gameplay::primitives::{
            resource::{Resource, ResourceSet},
            trade::{BankTrade, BankTradeKind, PlayerTrade},
        };

        assert_eq!(
            meaningful_event_line(&GameEvent::InitialResourcesGranted {
                player_id: P0,
                resources: ResourceSet {
                    brick: 1,
                    ore: 2,
                    ..ResourceSet::EMPTY
                },
            }),
            Some("p0 received initial resources [1B][2O]".to_owned())
        );
        assert_eq!(
            meaningful_event_line(&GameEvent::BankTradeCompleted {
                player_id: P0,
                trade: BankTrade {
                    give: Resource::Brick,
                    take: Resource::Wood,
                    kind: BankTradeKind::BankGeneric,
                },
            }),
            Some("p0 traded with bank: [4B] -> [1W]".to_owned())
        );
        assert_eq!(
            meaningful_event_line(&GameEvent::TradeOpened {
                session_id: catan_core::gameplay::game::trade::TradeSessionId(0),
                proposer_id: P0,
                offer_id: catan_core::gameplay::game::trade::TradeOfferId(0),
                offer: PlayerTrade {
                    give: ResourceSet {
                        sheep: 2,
                        ..ResourceSet::EMPTY
                    },
                    take: ResourceSet {
                        wheat: 1,
                        ..ResourceSet::EMPTY
                    },
                },
            }),
            Some("p0 opened public trade: give [2S] for [1H]".to_owned())
        );
        assert_eq!(
            meaningful_event_line(&GameEvent::PlayerDiscarded {
                player_id: P1,
                resources: ResourceSet {
                    wood: 1,
                    ..ResourceSet::EMPTY
                },
            }),
            Some("p1 discarded [1W]".to_owned())
        );
    }

    #[test]
    fn robber_move_and_resource_stolen_journal_entries_are_split() {
        use catan_core::gameplay::game::projection::GameProjection;
        use catan_core::gameplay::{
            game::{
                event::ObserverNotificationContext,
                state::SetupGameState,
                view::{ContextFactory, VisibilityConfig},
            },
            primitives::board::Tile,
        };

        let state = SetupGameState::default().finish();
        let index = catan_core::gameplay::game::index::GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let model = GameProjection::from_observer(
            ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        );
        let (hex, resource, number) = model
            .public
            .board
            .tiles
            .iter()
            .copied()
            .enumerate()
            .find_map(|(idx, tile)| match tile {
                Tile::Resource { resource, number } => Some((
                    catan_core::topology::HexIndex::spiral_to_hex(idx),
                    resource,
                    number,
                )),
                Tile::River { .. } | Tile::Desert => None,
            })
            .expect("default board should have a resource tile");

        assert_eq!(
            super::meaningful_event_line_with_model(
                &GameEvent::RobberMoved {
                    player_id: P0,
                    hex,
                    robbed_id: Some(P1),
                },
                Some(&model),
            ),
            Some(format!(
                "p0 moved the robber to {resource:?} {} hex",
                number.get()
            ))
        );
        assert_eq!(
            meaningful_event_line(&GameEvent::ResourceStolen {
                player_id: P0,
                robbed_id: P1,
                resource: catan_core::gameplay::primitives::resource::Resource::Brick,
            }),
            Some("p0 stole [?] from p1".to_owned())
        );
    }

    #[test]
    fn stolen_resource_journal_reveals_exact_card_only_to_involved_player_perspectives() {
        use catan_core::gameplay::game::projection::GameProjection;
        use catan_core::gameplay::{
            game::{
                event::ObserverNotificationContext,
                state::SetupGameState,
                view::{ContextFactory, VisibilityConfig},
            },
            primitives::resource::Resource,
        };

        let state = SetupGameState::default().finish();
        let index = catan_core::gameplay::game::index::GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let thief_model = GameProjection::from_observer(
            ObserverNotificationContext::Player {
                public: factory.public_view(visibility.player_policy(P0)),
                private: factory.private_view(P0),
            },
            false,
        );
        let victim_model = GameProjection::from_observer(
            ObserverNotificationContext::Player {
                public: factory.public_view(visibility.player_policy(P1)),
                private: factory.private_view(P1),
            },
            false,
        );
        let spectator_model = GameProjection::from_observer(
            ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        );
        let event = GameEvent::ResourceStolen {
            player_id: P0,
            robbed_id: P1,
            resource: Resource::Brick,
        };

        assert_eq!(
            super::meaningful_event_line_with_model(&event, Some(&thief_model)),
            Some("p0 stole [1B] from p1".to_owned())
        );
        assert_eq!(
            super::meaningful_event_line_with_model(&event, Some(&victim_model)),
            Some("p0 stole [1B] from p1".to_owned())
        );
        assert_eq!(
            super::meaningful_event_line_with_model(&event, Some(&spectator_model)),
            Some("p0 stole [?] from p1".to_owned())
        );
    }

    #[test]
    fn journal_keeps_recent_meaningful_entries_only() {
        let mut journal = EventJournal::new(2);
        journal.push_event(&GameEvent::DecisionClosed {
            decision_id: DecisionId(9),
        });
        journal.push_event(&GameEvent::DevCardUsed {
            player_id: P0,
            usage: catan_core::gameplay::primitives::dev_card::DevCardUsage::Knight {
                rob_hex: catan_core::topology::Hex::new(0, 0),
                robbed_id: Some(P1),
            },
        });
        journal.push_event(&GameEvent::DevCardUsed {
            player_id: P1,
            usage: catan_core::gameplay::primitives::dev_card::DevCardUsage::RoadBuild([
                catan_core::topology::Path::try_from((
                    catan_core::topology::Hex::new(0, 0),
                    catan_core::topology::Hex::new(1, 0),
                ))
                .expect("adjacent path"),
                catan_core::topology::Path::try_from((
                    catan_core::topology::Hex::new(1, 0),
                    catan_core::topology::Hex::new(1, -1),
                ))
                .expect("adjacent path"),
            ]),
        });
        journal.push_event(&GameEvent::DevCardUsed {
            player_id: P0,
            usage: catan_core::gameplay::primitives::dev_card::DevCardUsage::YearOfPlenty([
                catan_core::gameplay::primitives::resource::Resource::Brick,
                catan_core::gameplay::primitives::resource::Resource::Wood,
            ]),
        });

        let entries = journal
            .entries()
            .filter_map(|entry| match entry {
                JournalEntry::Event { text, .. } => Some(text),
                JournalEntry::Divider => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], "p1 used Road Building");
        assert_eq!(entries[1], "p0 used Year of Plenty for Brick and Wood");
    }

    #[test]
    fn journal_inserts_divider_between_different_player_moves() {
        let mut journal = EventJournal::new(8);
        journal.push_event(&GameEvent::DiceRolled {
            player_id: P0,
            value: dice_roll!(7),
        });
        journal.push_event(&GameEvent::DevCardBought { player_id: P0 });
        journal.push_event(&GameEvent::DiceRolled {
            player_id: P1,
            value: dice_roll!(8),
        });

        let entries = journal.entries().collect::<Vec<_>>();

        fn get_text(entry: &JournalEntry) -> Option<&str> {
            match entry {
                JournalEntry::Event { text, .. } => Some(text),
                JournalEntry::Divider => None,
            }
        }

        assert_eq!(entries.len(), 4);
        assert_eq!(get_text(entries[0]), Some("p0 rolled 7"));
        assert_eq!(entries[0].player_id(), Some(P0));
        assert_eq!(get_text(entries[1]), Some("p0 bought a development card"));
        assert!(entries[2].is_divider());
        assert_eq!(get_text(entries[3]), Some("p1 rolled 8"));
    }

    #[test]
    fn all_usable_dev_cards_have_user_facing_names() {
        for card in UsableDevCard::iter() {
            let event = GameEvent::DevCardUsed {
                player_id: P0,
                usage: match card {
                    UsableDevCard::Knight => {
                        catan_core::gameplay::primitives::dev_card::DevCardUsage::Knight {
                            rob_hex: catan_core::topology::Hex::new(0, 0),
                            robbed_id: None,
                        }
                    }
                    UsableDevCard::YearOfPlenty => {
                        catan_core::gameplay::primitives::dev_card::DevCardUsage::YearOfPlenty([
                            catan_core::gameplay::primitives::resource::Resource::Brick,
                            catan_core::gameplay::primitives::resource::Resource::Ore,
                        ])
                    }
                    UsableDevCard::RoadBuild => {
                        catan_core::gameplay::primitives::dev_card::DevCardUsage::RoadBuild([
                            catan_core::topology::Path::try_from((
                                catan_core::topology::Hex::new(0, 0),
                                catan_core::topology::Hex::new(1, 0),
                            ))
                            .expect("adjacent path"),
                            catan_core::topology::Path::try_from((
                                catan_core::topology::Hex::new(1, 0),
                                catan_core::topology::Hex::new(1, -1),
                            ))
                            .expect("adjacent path"),
                        ])
                    }
                    UsableDevCard::Monopoly => {
                        catan_core::gameplay::primitives::dev_card::DevCardUsage::Monopoly(
                            catan_core::gameplay::primitives::resource::Resource::Sheep,
                        )
                    }
                },
            };

            assert!(meaningful_event_line(&event).is_some());
        }
    }
}
