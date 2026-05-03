//! Human-readable labels for game objects.
//!
//! Formats board coordinates, intersections, builds, and bank trades for prompts,
//! menus, and current-selection displays.

use catan_core::{
    gameplay::primitives::{
        build::{Build, EstablishmentType},
        trade::{BankTrade, BankTradeKind},
    },
    topology::{Hex, Intersection, Path as BoardPath},
};

pub(crate) fn hex_label(hex: Hex) -> String {
    format!("hex {} (q={}, r={})", hex.index().to_spiral(), hex.q, hex.r)
}

pub(crate) fn path_label(path: BoardPath) -> String {
    let (a, b) = path.as_pair();
    format!(
        "road {} {} ({},{} -> {},{})",
        a.index().to_spiral(),
        b.index().to_spiral(),
        a.q,
        a.r,
        b.q,
        b.r
    )
}

pub(crate) fn intersection_label(intersection: Intersection) -> String {
    let label = intersection
        .as_set()
        .into_iter()
        .map(|hex| hex.index().to_spiral().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    format!("intersection {label}")
}

pub(crate) fn build_label(build: Build) -> String {
    match build {
        Build::Road(road) => path_label(road.pos),
        Build::Establishment(establishment) => match establishment.stage {
            EstablishmentType::Settlement => {
                format!("settlement {}", intersection_label(establishment.pos))
            }
            EstablishmentType::City => format!("city {}", intersection_label(establishment.pos)),
        },
    }
}

pub(crate) fn bank_trade_label(trade: BankTrade) -> String {
    let rate = match trade.kind {
        BankTradeKind::BankGeneric => "4:1",
        BankTradeKind::PortGeneric => "3:1",
        BankTradeKind::PortSpecific => "2:1",
    };
    format!("{rate} {:?} -> {:?}", trade.give, trade.take)
}
