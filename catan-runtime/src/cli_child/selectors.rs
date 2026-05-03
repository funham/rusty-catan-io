//! Selection helpers backed by host-provided legal options.
//!
//! Contains pure helpers for board navigation and for deriving selectable settlements,
//! roads, builds, robber targets, and road-building choices from `LegalDecisionOptions`.

use std::collections::BTreeSet;

use catan_agents::remote_agent::{LegalDecisionOptions, UiModel};
use catan_core::gameplay::primitives::{
    build::{Build, Road},
    dev_card::DevCardUsage,
    player::PlayerId,
};
use catan_core::topology::{Hex, HexIndex, Intersection, Path as BoardPath};
use catan_render::field::SelectionStatus;
use crossterm::event::KeyCode;

use super::input::PartialBuildMode;

pub(crate) fn selection_status(is_available: bool) -> SelectionStatus {
    if is_available {
        SelectionStatus::Available
    } else {
        SelectionStatus::Unavailable
    }
}

pub(crate) fn move_hex_by_key(current: Hex, key: KeyCode, board_hexes: &BTreeSet<Hex>) -> Hex {
    let next = match key {
        KeyCode::Left => Hex::new(current.q - 1, current.r),
        KeyCode::Right => Hex::new(current.q + 1, current.r),
        KeyCode::Up => Hex::new(current.q, current.r - 1),
        KeyCode::Down => Hex::new(current.q, current.r + 1),
        _ => current,
    };

    if board_hexes.contains(&next) {
        next
    } else {
        current
    }
}

fn board_hexes(model: &UiModel) -> Vec<Hex> {
    (0..model.public.board.tiles.len())
        .map(HexIndex::spiral_to_hex)
        .collect()
}

pub(crate) fn board_hex_set(model: &UiModel) -> BTreeSet<Hex> {
    board_hexes(model).into_iter().collect()
}

pub(crate) fn legal_initial_settlements(legal: &LegalDecisionOptions) -> BTreeSet<Intersection> {
    legal
        .initial_placements
        .iter()
        .map(|placement| placement.establishment_position)
        .collect()
}

pub(crate) fn initial_roads_for_settlement(
    legal: &LegalDecisionOptions,
    settlement: Intersection,
) -> Vec<Road> {
    legal
        .initial_placements
        .iter()
        .filter(|placement| placement.establishment_position == settlement)
        .map(|placement| placement.road)
        .collect()
}

pub(crate) fn legal_builds_for_mode(
    legal: &LegalDecisionOptions,
    mode: PartialBuildMode,
) -> Vec<Build> {
    match mode {
        PartialBuildMode::Settlement => legal.builds.settlements.clone(),
        PartialBuildMode::Road => legal.builds.roads.clone(),
        PartialBuildMode::City => legal.builds.cities.clone(),
    }
}

pub(crate) fn knight_hexes(legal: &LegalDecisionOptions) -> BTreeSet<Hex> {
    legal
        .dev_card_usages
        .iter()
        .filter_map(|usage| match usage {
            DevCardUsage::Knight { rob_hex, .. } => Some(*rob_hex),
            _ => None,
        })
        .collect()
}

pub(crate) fn knight_rob_targets(legal: &LegalDecisionOptions, hex: Hex) -> Vec<PlayerId> {
    legal
        .dev_card_usages
        .iter()
        .filter_map(|usage| match usage {
            DevCardUsage::Knight {
                rob_hex,
                robbed_id: Some(robbed_id),
            } if *rob_hex == hex => Some(*robbed_id),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn roadbuild_first_options(legal: &LegalDecisionOptions) -> Vec<Build> {
    legal
        .dev_card_usages
        .iter()
        .filter_map(|usage| match usage {
            DevCardUsage::RoadBuild([first, _]) => Some(*first),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|pos| Build::Road(Road { pos }))
        .collect()
}

pub(crate) fn roadbuild_second_options(
    legal: &LegalDecisionOptions,
    first: BoardPath,
) -> Vec<Build> {
    legal
        .dev_card_usages
        .iter()
        .filter_map(|usage| match usage {
            DevCardUsage::RoadBuild([candidate_first, second]) if *candidate_first == first => {
                Some(*second)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|pos| Build::Road(Road { pos }))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crossterm::event::KeyCode;

    use super::{Hex, move_hex_by_key};

    fn hex_set(hexes: impl IntoIterator<Item = Hex>) -> BTreeSet<Hex> {
        hexes.into_iter().collect()
    }

    #[test]
    fn hex_arrow_navigation_changes_q_and_r_coordinates() {
        let board = hex_set([
            Hex::new(0, 0),
            Hex::new(1, 0),
            Hex::new(-1, 0),
            Hex::new(0, -1),
            Hex::new(0, 1),
        ]);

        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Right, &board),
            Hex::new(1, 0)
        );
        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Left, &board),
            Hex::new(-1, 0)
        );
        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Up, &board),
            Hex::new(0, -1)
        );
        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Down, &board),
            Hex::new(0, 1)
        );
    }

    #[test]
    fn hex_arrow_navigation_stays_on_board() {
        let board = hex_set([Hex::new(0, 0)]);

        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Right, &board),
            Hex::new(0, 0)
        );
    }
}
