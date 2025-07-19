use crate::Hex;
use crate::common::*;
use crate::player::*;
use crate::topology::*;

pub const HEX_COUNT: usize = 19;
pub const VERT_COUNT: usize = 54;
pub const EDGE_COUNT: usize = 72;

pub struct Field<const P_COUNT: usize> {
    hexes: [Hex; HEX_COUNT],
    vertices: [Vertex; VERT_COUNT],
    edges: [Edge; EDGE_COUNT],
    players: [Player; P_COUNT],
}

impl<const P_COUNT: usize> Field<P_COUNT> {
    pub fn new(hexes: [Hex; HEX_COUNT]) -> Self {
        let vertices = Self::make_vertices(&hexes);
        let edges = Self::make_edges(&hexes, &vertices);
        let players = Self::make_players();

        Self {
            hexes,
            vertices,
            edges,
            players,
        }
    }

    fn make_vertices(hexes: &[Hex; HEX_COUNT]) -> [Vertex; VERT_COUNT] {
        todo!()
    }

    fn make_edges(hexes: &[Hex; HEX_COUNT], vertices: &[Vertex; VERT_COUNT]) -> [Edge; EDGE_COUNT] {
        todo!()
    }

    fn make_players() -> [Player; P_COUNT] {
        todo!()
    }
}

enum EdgeContextCreationError {}

// impl EdgeContext {
//     pub fn try_new<const P_COUNT: usize>(
//         field: &Field<P_COUNT>,
//         from: VertexId,
//         to: VertexId,
//     ) -> Result<Self, EdgeContextCreationError> {
//         let hexes_from = field.vertices.get(from).unwrap().hexes;
//         let hexes_to = field.vertices.get(to).unwrap().hexes;
//         let intersection = hexes_from
//             .intersection(&hexes_to)
//             .cloned()
//             .collect::<BTreeSet<_>>();

//         let x_hexes_from = hexes_from
//             .difference(&intersection)
//             .cloned()
//             .collect::<BTreeSet<_>>();
//         let x_hexes_to = hexes_to
//             .difference(&intersection)
//             .cloned()
//             .collect::<BTreeSet<_>>();

        
//     }
// }
