use catan_core::gameplay::{
    field::{
        PortPos,
        state::{BoardLayout, BoardState},
    },
    game::view::PublicGameView,
    primitives::{
        PortKind, Tile,
        build::{Establishment, Road},
        player::PlayerId,
    },
};

#[derive(Debug, Clone)]
pub struct RenderGameView {
    pub board: RenderBoard,
    pub board_state: BoardState,
    pub builds: Vec<RenderPlayerBuilds>,
}

#[derive(Debug, Clone)]
pub struct RenderBoard {
    pub n_players: usize,
    pub field_radius: u8,
    pub tiles: Vec<Tile>,
    pub ports: Vec<(PortPos, PortKind)>,
}

#[derive(Debug, Clone)]
pub struct RenderPlayerBuilds {
    pub player_id: PlayerId,
    pub establishments: Vec<Establishment>,
    pub roads: Vec<Road>,
}

impl RenderBoard {
    pub fn from_board(board: &BoardLayout) -> Self {
        Self {
            n_players: board.n_players,
            field_radius: board.arrangement.radius(),
            tiles: board.arrangement.iter().collect(),
            ports: board
                .arrangement
                .ports()
                .iter()
                .map(|(path, port)| (*path, *port))
                .collect(),
        }
    }
}

impl<'a> From<&PublicGameView<'a>> for RenderGameView {
    fn from(value: &PublicGameView<'a>) -> Self {
        Self {
            board: RenderBoard::from_board(value.board),
            board_state: *value.board_state,
            builds: value
                .builds
                .players_indexed()
                .map(|(player_id, builds)| RenderPlayerBuilds {
                    player_id,
                    establishments: builds.establishments.iter().copied().collect(),
                    roads: builds.roads.iter().collect(),
                })
                .collect(),
        }
    }
}
