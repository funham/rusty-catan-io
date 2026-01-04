use super::player::PlayerId;
use num::Integer;
use std::marker::PhantomData;

#[derive(Debug)]
pub struct RegularCycle;
#[derive(Debug)]
pub struct BackAndForthCycle;

#[derive(Debug)]
pub struct GameTurn<CycleType = RegularCycle> {
    n_players: u8, // in [0..=4]
    rounds_played: u16,
    turn_index: u8, // in [0..=n_players]
    _p: PhantomData<CycleType>,
}

impl<CycleType> GameTurn<CycleType> {
    /// Create `GameTurn` object with specified number of players
    pub fn new(n_players: u8) -> Self {
        Self {
            n_players,
            rounds_played: 0,
            turn_index: 0,
            _p: PhantomData::default(),
        }
    }

    pub fn new_with_initial_index(n_players: u8, initial_index: u8) -> Option<Self> {
        if initial_index >= n_players {
            return None;
        }

        Some(Self {
            n_players,
            rounds_played: 0,
            turn_index: initial_index,
            _p: PhantomData::default(),
        })
    }

    pub fn get_rounds_played(&self) -> u16 {
        self.rounds_played
    }

    pub fn get_turn_index(&self) -> usize {
        self.turn_index as usize
    }
}

impl GameTurn<RegularCycle> {
    /// Pass to the next player
    ///
    /// # Examples
    /// ~~~
    /// use catan_core::gameplay::primitives::turn::*;
    ///
    /// let mut turn = GameTurn::<RegularCycle>::new(3);
    /// turn.next();
    /// turn.next();
    /// assert_eq!(turn.get_turn_index(), 2);
    /// assert_eq!(turn.get_rounds_played(), 0);
    /// turn.next();
    /// assert_eq!(turn.get_turn_index(), 0);
    /// assert_eq!(turn.get_rounds_played(), 1);
    /// ~~~
    pub fn next(&mut self) {
        self.turn_index.inc();

        let (round_played, index_truncated) = self.turn_index.div_mod_floor(&self.n_players);
        self.rounds_played += round_played as u16;
        self.turn_index = index_truncated;
    }
}

impl GameTurn<BackAndForthCycle> {
    /// Pass to the next player
    ///
    /// # Examples
    /// ~~~
    /// use catan_core::gameplay::primitives::turn::*;
    ///
    /// let mut turn = GameTurn::<BackAndForthCycle>::new(3);
    /// turn.next();
    /// turn.next();
    /// assert_eq!(turn.get_turn_index(), 2);
    /// assert_eq!(turn.get_rounds_played(), 0);
    /// turn.next();
    /// assert_eq!(turn.get_turn_index(), 2);
    /// assert_eq!(turn.get_rounds_played(), 1);
    /// turn.next();
    /// assert_eq!(turn.get_turn_index(), 1);
    /// assert_eq!(turn.get_rounds_played(), 1);
    /// turn.next();
    /// assert_eq!(turn.get_turn_index(), 0);
    /// assert_eq!(turn.get_rounds_played(), 1);
    /// turn.next();
    /// assert_eq!(turn.get_turn_index(), 0);
    /// assert_eq!(turn.get_rounds_played(), 2);
    /// ~~~
    pub fn next(&mut self) {
        let incremented = self.turn_index as i32
            + match self.rounds_played {
                even if even.is_even() => 1,
                _ => -1,
            };

        if (0..self.n_players as i32).contains(&incremented) {
            self.turn_index = incremented as u8;
        } else {
            self.rounds_played.inc();
        }
    }
}

impl<T> Into<PlayerId> for GameTurn<T> {
    fn into(self) -> PlayerId {
        self.get_turn_index() as PlayerId
    }
}
