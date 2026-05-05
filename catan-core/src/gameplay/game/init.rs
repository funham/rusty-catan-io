use std::sync::Arc;

use crate::gameplay::{
    field::state::{BoardLayout, BoardState, FieldBuildParam},
    game::state::GameState,
    primitives::{
        bank::Bank,
        build::BoardBuildData,
        player::PlayerDataContainer,
        turn::{BackAndForthCycle, GameTurn},
    },
};

#[derive(Clone)]
pub struct GameInitializationState {
    pub board: Arc<BoardLayout>,
    pub board_state: BoardState,
    pub turn: GameTurn<BackAndForthCycle>,
    pub bank: Bank,
    pub players: PlayerDataContainer,
    pub builds: BoardBuildData,
}

impl Default for GameInitializationState {
    fn default() -> Self {
        Self::new(FieldBuildParam::default())
    }
}

impl GameInitializationState {
    pub fn new(field_build_param: FieldBuildParam) -> Self {
        let board = Arc::new(BoardLayout::new(field_build_param));
        let mut bank = Bank::default();
        bank.shuffle_dev_cards();
        Self::from_board_and_bank(board, bank)
    }

    pub fn new_with_seed(field_build_param: FieldBuildParam, seed: u64) -> Self {
        let board = Arc::new(BoardLayout::new(field_build_param));
        let mut bank = Bank::default();
        bank.shuffle_dev_cards_with_seed(seed);
        Self::from_board_and_bank(board, bank)
    }

    fn from_board_and_bank(board: Arc<BoardLayout>, bank: Bank) -> Self {
        Self {
            turn: GameTurn::new(board.n_players as u8),
            players: PlayerDataContainer::new(board.n_players),
            builds: BoardBuildData::new(board.n_players),
            board_state: BoardState::new(&board),
            board,
            bank,
        }
    }

    pub fn finish(self) -> GameState {
        GameState {
            board: self.board,
            board_state: self.board_state,
            turn: self.turn.into_regular(),
            bank: self.bank,
            players: self.players,
            builds: self.builds,
        }
    }
}
