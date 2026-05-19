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
    random::GameRandom,
};

#[derive(Debug, Clone)]
pub struct GameInitializationState {
    pub board: Arc<BoardLayout>,
    pub board_state: BoardState,
    pub turn: GameTurn<BackAndForthCycle>,
    pub bank: Bank,
    pub players: PlayerDataContainer,
    pub builds: BoardBuildData,
}

#[derive(Debug)]
pub struct GameInitializationOptions {
    pub random: GameRandom,
}

impl Default for GameInitializationOptions {
    fn default() -> Self {
        Self {
            random: GameRandom::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_initialization_shuffles_dev_cards_deterministically() {
        let first = GameInitializationState::new_with_seed(FieldBuildParam::default(), 42);
        let second = GameInitializationState::new_with_seed(FieldBuildParam::default(), 42);
        let different = GameInitializationState::new_with_seed(FieldBuildParam::default(), 43);

        assert_eq!(first.bank.dev_cards, second.bank.dev_cards);
        assert_ne!(first.bank.dev_cards, different.bank.dev_cards);
    }
}

impl Default for GameInitializationState {
    fn default() -> Self {
        Self::new(FieldBuildParam::default())
    }
}

impl GameInitializationState {
    pub fn new(field_build_param: FieldBuildParam) -> Self {
        Self::new_with_options(field_build_param, GameInitializationOptions::default())
    }

    pub fn new_with_options(
        field_build_param: FieldBuildParam,
        mut options: GameInitializationOptions,
    ) -> Self {
        let board = Arc::new(BoardLayout::new(field_build_param));
        let mut bank = Bank::default();
        options.random.shuffle_dev_cards(&mut bank.dev_cards);
        Self::from_board_and_bank(board, bank)
    }

    pub fn new_with_seed(field_build_param: FieldBuildParam, seed: u64) -> Self {
        Self::new_with_options(
            field_build_param,
            GameInitializationOptions {
                random: GameRandom::seeded(seed),
            },
        )
    }

    fn from_board_and_bank(board: Arc<BoardLayout>, bank: Bank) -> Self {
        Self {
            turn: GameTurn::new(board.n_players as u8),
            players: PlayerDataContainer::new(board.n_players),
            builds: BoardBuildData::new(board.n_players),
            board_state: BoardState::new(&board),
            bank,
            board,
        }
    }

    pub fn finish(self) -> GameState {
        GameState {
            table: super::state::TableState {
                board: self.board,
                board_state: self.board_state,
                bank: self.bank,
                players: self.players,
                builds: self.builds,
            },
            turn: self.turn.into_regular(),
        }
    }

    pub fn into_setup_parts(self) -> (super::state::TableState, GameTurn<BackAndForthCycle>) {
        (
            super::state::TableState {
                board: self.board,
                board_state: self.board_state,
                bank: self.bank,
                players: self.players,
                builds: self.builds,
            },
            self.turn,
        )
    }
}
