// catan-core::algorithm
// ---
// a module for storing implementations of algorithms
// that are used repeatedly across the codebase,
// using minimal abstraction level.

use std::collections::BTreeMap;

use crate::{
    common::SmallSet,
    gameplay::{
        constants::capacities::PLAYER_PORTS_INLINE,
        game::{event::ResourceDistribution, state::GameState},
        primitives::{
            PortKind, Tile,
            build::{BoardBuildData, PlayerBuildData},
            player::{PlayerDataContainer, PlayerId},
        },
    },
    math::dice::TileNum,
    topology::{Hex, Intersection},
};

pub fn player_order_from(
    start_id: PlayerId,
    player_count: usize,
) -> impl Iterator<Item = PlayerId> {
    (start_id.index()..player_count)
        .chain(0..start_id.index())
        .map(|id| PlayerId::try_from(id).expect("player count should fit in u8"))
}

pub fn is_player_on_hex(hex: Hex, builds: &PlayerBuildData) -> bool {
    builds
        .establishments
        .iter()
        .any(|establishment| establishment.vtx.as_arr().contains(&hex))
}

pub fn players_on_hex<'a>(
    hex: Hex,
    builds: impl Iterator<Item = &'a PlayerBuildData>,
) -> impl Iterator<Item = PlayerId> {
    builds.enumerate().filter_map(move |(id, builds)| {
        is_player_on_hex(hex, builds)
            .then(|| PlayerId::try_from(id).expect("player count should fit in u8"))
    })
}

pub fn robbery_candidates<'a>(
    rob_hex: Hex,
    robber_id: PlayerId,
    builds: &'a BoardBuildData,
    players: &'a PlayerDataContainer,
) -> impl Iterator<Item = PlayerId> + use<'a> {
    builds
        .query()
        .builds_on_hex(rob_hex)
        .into_iter()
        .filter(move |(id, builds)| {
            *id != robber_id
                && !builds.establishments.is_empty()
                && !players.get(*id).resources().is_empty()
        })
        .map(|(id, _)| id)
}

pub fn resource_distribution_for_roll(
    state: &GameState,
    start_player: PlayerId,
    num: TileNum,
) -> ResourceDistribution {
    let hexes = state.board.hexes_by_num(num);
    let mut bank_resources = state.bank.resources;
    let mut by_player = ResourceDistribution::new();

    for player_id in player_order_from(start_player, state.players.count()) {
        for establishment in &state.builds[player_id].establishments {
            let adjacent = establishment.vtx.as_set();

            for hex in hexes.iter().filter(|hex| adjacent.contains(hex)) {
                if *hex == state.board_state.robber_pos {
                    continue;
                }
                if let Tile::Resource { resource, .. } = state.board.arrangement[*hex] {
                    let resources = (resource, establishment.stage.harvest_amount() as u16).into();
                    if bank_resources.subtract_in_place(&resources).is_ok() {
                        add_distribution(&mut by_player, player_id, resources);
                    }
                }
            }
        }
    }

    by_player
}

fn add_distribution(
    by_player: &mut ResourceDistribution,
    player_id: PlayerId,
    resources: crate::gameplay::primitives::resource::ResourceSet,
) {
    if let Some((_, existing)) = by_player
        .iter_mut()
        .find(|(existing_id, _)| *existing_id == player_id)
    {
        *existing += &resources;
    } else {
        by_player.push((player_id, resources));
    }
}

pub fn get_ports_acquired(
    ports: &BTreeMap<Intersection, PortKind>,
    builds: &BoardBuildData,
) -> Vec<SmallSet<PortKind, PLAYER_PORTS_INLINE>> {
    let mut result = Vec::new();
    for id in 0..builds.players().len() {
        let mut set = SmallSet::new();

        let player_id = PlayerId::try_from(id).expect("player count should fit in u8");
        for est in builds.by_player(player_id).establishments.iter() {
            if let Some(port) = ports.get(&est.vtx) {
                set.insert(*port);
            }
        }
        result.push(set);
    }

    result
}
