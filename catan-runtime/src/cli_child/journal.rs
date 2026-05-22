//! User-facing event journal formatting for the CLI.

use std::collections::VecDeque;

use catan_core::gameplay::{
    game::{event::GameEvent, run::GameResult, trade::TradeResponseState},
    primitives::{
        PlayerId,
        build::{Build, EstablishmentType},
        dev_card::DevCardUsage,
        resource::{Resource, ResourceSet},
        trade::{BankTrade, BankTradeKind, PlayerTrade},
    },
};

#[derive(Debug, Clone)]
pub(crate) struct EventJournal {
    entries: VecDeque<JournalEntry>,
    capacity: usize,
    last_player_id: Option<PlayerId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JournalEntry {
    Event {
        text: String,
        player_id: Option<PlayerId>,
    },
    Divider,
}

impl JournalEntry {
    #[allow(dead_code)]
    pub(crate) fn is_divider(&self) -> bool {
        matches!(self, Self::Divider)
    }

    #[allow(dead_code)]
    pub(crate) fn is_event(&self) -> bool {
        matches!(self, Self::Event { .. })
    }

    #[allow(dead_code)]
    pub(crate) fn player_id(&self) -> Option<PlayerId> {
        match self {
            JournalEntry::Event { player_id, .. } => *player_id,
            JournalEntry::Divider => None,
        }
    }
}

impl EventJournal {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
            last_player_id: None,
        }
    }

    pub(crate) fn push_event(&mut self, event: &GameEvent) -> Option<String> {
        let line = meaningful_event_line(event)?;
        let player_id = event_player_id(event);

        let needs_divider = match (self.last_player_id, event_player_id(event)) {
            (Some(last_id), Some(curr_id)) => curr_id != last_id,
            _ => false,
        };

        if needs_divider {
            self.push_entry(JournalEntry::Divider);
        }

        self.push_entry(JournalEntry::Event {
            text: line.clone(),
            player_id,
        });

        self.last_player_id = player_id;

        Some(line)
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

    pub(crate) fn entries(&self) -> impl DoubleEndedIterator<Item = &JournalEntry> {
        self.entries.iter()
    }
}

pub(crate) fn meaningful_event_line(event: &GameEvent) -> Option<String> {
    Some(match event {
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
            resource_set_label(resources)
        ),
        GameEvent::DiceRolled { player_id, value } => {
            format!("p{player_id} rolled {}", value.as_u8())
        }
        GameEvent::ResourcesDistributed { by_player } => {
            let grants = by_player
                .iter()
                .filter(|(_, resources)| !resources.is_empty())
                .map(|(player_id, resources)| {
                    format!("p{player_id} {}", resource_set_label(resources))
                })
                .collect::<Vec<_>>();
            if grants.is_empty() {
                "no resources were produced".to_owned()
            } else {
                format!("resources: {}", grants.join("; "))
            }
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
        } => format!("p{player_id} discarded {}", resource_set_label(resources)),
        GameEvent::RobberMoved {
            player_id,
            hex,
            robbed_id,
        } => match robbed_id {
            Some(robbed_id) => format!(
                "p{player_id} moved the robber to h{} and targeted p{robbed_id}",
                hex.index().to_spiral()
            ),
            None => format!(
                "p{player_id} moved the robber to h{}",
                hex.index().to_spiral()
            ),
        },
        GameEvent::ResourceStolen {
            player_id,
            robbed_id,
            resource,
        } => format!("p{player_id} stole {resource:?} from p{robbed_id}"),
        GameEvent::GameFinished { result, .. } => match result {
            GameResult::Win(player_id) => format!("p{player_id} won the game"),
            GameResult::Interrupted { reason } => format!("game interrupted: {reason}"),
            GameResult::LimitReached { turns } => format!("turn limit reached after {turns} turns"),
        },
        GameEvent::DecisionOpened(_)
        | GameEvent::DecisionClosed { .. }
        | GameEvent::CommandRejected { .. }
        | GameEvent::DevCardDrawn { .. } => return None,
    })
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
        GameEvent::GameFinished { result, .. } => match result {
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
        BankTradeKind::BankGeneric => "4:1",
        BankTradeKind::PortGeneric => "3:1",
        BankTradeKind::PortSpecific => "2:1",
    };
    format!("{rate} {:?} -> {:?}", trade.give, trade.take)
}

fn player_trade_label(trade: &PlayerTrade) -> String {
    format!(
        "give {} for {}",
        resource_set_label(&trade.give),
        resource_set_label(&trade.take)
    )
}

fn resource_set_label(resources: &ResourceSet) -> String {
    let parts = Resource::iter()
        .filter_map(|resource| {
            let count = resources[resource];
            (count > 0).then(|| format!("{count} {resource:?}"))
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "nothing".to_owned()
    } else {
        parts.join(", ")
    }
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
        gameplay::{
            game::{
                decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision},
                event::GameEvent,
            },
            primitives::{dev_card::UsableDevCard, player::PlayerId},
        },
        math::dice::DiceRoll,
    };

    use crate::cli_child::journal::JournalEntry;

    use super::{EventJournal, meaningful_event_line};

    const P0: PlayerId = PlayerId::new(0);
    const P1: PlayerId = PlayerId::new(1);

    #[test]
    fn journal_formats_meaningful_player_events() {
        let dice = GameEvent::DiceRolled {
            player_id: P1,
            value: DiceRoll::seven(),
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
    fn journal_drops_technical_decision_events() {
        let technical = GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(4),
            player_id: P0,
            kind: DecisionKind::RegularCommand,
            lifetime: DecisionLifetime::OneShot,
        });

        assert_eq!(meaningful_event_line(&technical), None);
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
            value: DiceRoll::seven(),
        });
        journal.push_event(&GameEvent::DevCardBought { player_id: P0 });
        journal.push_event(&GameEvent::DiceRolled {
            player_id: P1,
            value: DiceRoll::eight(),
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
