// catan-core::algorithm
// ---
// a module for storing implementations of algorithms
// that are used repeatedly across the codebase,
// using minimal abstraction level.

use crate::{
    gameplay::primitives::{build::PlayerBuildData, player::PlayerId},
    topology::Hex,
};

pub fn is_player_on_hex(hex: Hex, builds: &PlayerBuildData) -> bool {
    for v in hex.vertices() {
        let has_build_on_intersection = builds.establishments.iter().any(|est| est.pos == v);
        if has_build_on_intersection {
            return true;
        }
    }

    false
}

pub fn players_on_hex<'a>(
    hex: Hex,
    builds: impl Iterator<Item = &'a PlayerBuildData>,
) -> impl IntoIterator<Item = PlayerId> {
    builds
        .enumerate()
        .filter_map(|(id, builds)| is_player_on_hex(hex, &builds).then_some(id))
        .collect::<Vec<_>>()
}
