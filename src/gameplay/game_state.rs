use std::collections::BTreeMap;
use std::default;
use std::rc::Rc;
use std::{cell::RefCell, marker::PhantomData};

use num::Integer;

use crate::gameplay::field::GameInitField;
use crate::{
    gameplay::{
        dev_card::DevCardKind,
        field::Field,
        move_request::{DevCardUsage, RobRequest},
        player::{OpponentData, Player, PlayerData, PlayerId},
        resource::{Resource, ResourceCollection},
        strategy::Strategy,
    },
    math::dice::DiceRoller,
    topology::Edge,
};

pub struct RegularCycle;
pub struct BackAndForthCycle;
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
    /// use rusty_catan_io::gameplay::game_state::*;
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
    /// use rusty_catan_io::gameplay::game_state::*;
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

pub struct Bank {
    resources: ResourceCollection,
    dev_cards: Vec<DevCardKind>,
}

impl Bank {
    pub fn buy_dev_card(&mut self, account: &mut ResourceCollection) -> Option<DevCardKind> {
        if self.dev_cards.is_empty() {
            return None;
        }
        todo!();
        self.dev_cards.pop()
    }
}

pub struct BankResourceExchange {
    pub to_bank: ResourceCollection,
    pub from_bank: ResourceCollection,
    pub player_id: PlayerId,
}

pub enum BankResourceExchangeError {
    BankIsShort,
    AccountIsShort,
}

struct Transfer<'a> {
    from: &'a mut ResourceCollection,
    to: &'a mut ResourceCollection,
    resources: ResourceCollection,
}

impl<'a> Transfer<'a> {
    fn new(
        from: &'a mut ResourceCollection,
        to: &'a mut ResourceCollection,
        resources: ResourceCollection,
    ) -> Self {
        Self {
            from,
            to,
            resources,
        }
    }

    fn execute(self) {
        todo!()
    }
}

pub struct PlayerTrade {
    pub give: ResourceCollection,
    pub take: ResourceCollection,
}

impl PlayerTrade {
    pub fn opposite(&self) -> Self {
        Self {
            give: self.take,
            take: self.give,
        }
    }
}

pub struct GameInitializationState {
    pub field: GameInitField,
    pub turn: GameTurn<BackAndForthCycle>,
    pub strats: Vec<Rc<RefCell<dyn Strategy>>>,
}

pub struct GameState {
    pub(super) field: Field,
    pub(super) dice: Box<dyn DiceRoller>,
    pub(super) bank: Bank,
    pub(super) players: Vec<Player>,
    pub(super) turn: GameTurn,
}

/// player's perspective on a game, used in `Strategy`
pub struct Perspective<'a> {
    pub player_data: &'a PlayerData,
    pub field: &'a Field,
    pub bank: &'a Bank,
    pub opponents: BTreeMap<PlayerId, OpponentData>,
}

impl<'a> Perspective<'a> {}

/// convinient struct with neccessary info about player who's turn it currently is
#[derive(Debug)]
pub struct TurnHandlingParams {
    pub(super) player_id: PlayerId,
    pub(super) strategy: Rc<RefCell<dyn Strategy>>,
}

impl GameInitializationState {}

impl GameState {
    pub fn get_perspective(&self, player_id: PlayerId) -> Perspective {
        let opponents = self
            .players
            .iter()
            .enumerate()
            .filter(|(i, _)| i != &player_id)
            .map(|(i, p)| (i, OpponentData::from(&p.data)))
            .collect::<BTreeMap<PlayerId, OpponentData>>();

        Perspective {
            player_data: &self.players[player_id].data,
            field: &self.field,
            bank: &self.bank,
            opponents,
        }
    }
    pub fn get_params(&self) -> TurnHandlingParams {
        let player_id = self.turn.get_turn_index();
        TurnHandlingParams {
            player_id,
            strategy: self.players[player_id].strategy.clone(),
        }
    }
    pub fn bank_resource_exchange<'a>(
        &mut self,
        exhange: BankResourceExchange,
    ) -> Result<(), BankResourceExchangeError> {
        let account = &mut self.players[exhange.player_id].data.resources;
        match (
            self.bank.resources - &exhange.from_bank,
            *account - &exhange.to_bank,
        ) {
            (None, _) => Err(BankResourceExchangeError::BankIsShort),
            (_, None) => Err(BankResourceExchangeError::AccountIsShort),
            (Some(bank_res), Some(acc_res)) => Ok({
                self.bank.resources = bank_res;
                *account = acc_res;
            }),
        }
    }
    pub fn transfer_to_bank(
        &mut self,
        resources: ResourceCollection,
        player_id: PlayerId,
    ) -> Result<(), BankResourceExchangeError> {
        self.bank_resource_exchange(BankResourceExchange {
            to_bank: resources,
            from_bank: ResourceCollection::default(),
            player_id,
        })
    }
    pub fn pay_to_player(
        &mut self,
        resources: ResourceCollection,
        player_id: PlayerId,
    ) -> Result<(), BankResourceExchangeError> {
        self.bank_resource_exchange(BankResourceExchange {
            to_bank: ResourceCollection::default(),
            from_bank: resources,
            player_id,
        })
    }

    pub fn player_ids_starting_from(
        &self,
        start_id: PlayerId,
    ) -> impl IntoIterator<Item = PlayerId> + use<> {
        (start_id..self.players.len())
            .chain(0..start_id)
            .collect::<Vec<_>>()
    }

    // TODO: add validation (return Result<(), ...>)
    pub fn move_robbers(&mut self, pos: crate::topology::hex::Hex) {
        self.field.robber_pos = pos;
    }

    // TODO: add validation
    pub fn rob(&mut self, rob_request: RobRequest, robber_id: PlayerId) {
        if let Some(robbed_id) = rob_request.player {
            let (left, right) = self.players.split_at_mut(robbed_id.max(robber_id));
            let left_len = left.len();
            let ((robbed_half, robbed_id), (robber_half, robber_id)) = match robbed_id
                .cmp(&robber_id)
            {
                std::cmp::Ordering::Equal => unreachable!("you can't rob yourself"),
                std::cmp::Ordering::Less => ((left, robbed_id), (right, robber_id - left_len)),
                std::cmp::Ordering::Greater => ((right, robbed_id - left_len), (left, robber_id)),
            };

            let robbed_account = &mut robbed_half[robbed_id].data.resources;
            let robber_account = &mut robber_half[robber_id].data.resources;

            // let player_to_rob = &mut self.players[id];
            let stolen = robbed_account.peek_random();
            if let Some(card) = stolen {
                Transfer::new(robbed_account, robber_account, card.into());
            }
        }
    }

    /// goes through the players and if one have >9 vp returns it
    pub fn check_win_condition(&self) -> Option<PlayerId> {
        todo!()
    }

    // TODO: add validation
    pub fn use_dev_card(&mut self, usage: DevCardUsage, user: PlayerId) {
        match usage {
            DevCardUsage::Knight(rob_request) => {
                self.rob(rob_request, user);
            }
            DevCardUsage::YearOfPlenty(list) => {
                self.use_year_of_plenty(list, user);
            }
            DevCardUsage::RoadBuild(x) => {
                self.use_roadbuild(x, user);
            }
            DevCardUsage::Monopoly(resource) => {
                self.use_monopoly(resource, user);
            }
        }

        self.players[user]
            .data
            .dev_cards
            .move_to_played(usage.card());
    }

    /* helper functions used only by `use_dev_card` */

    // TODO: add validation
    fn use_year_of_plenty(
        &mut self,
        list: (Resource, Resource),
        player: PlayerId,
    ) -> ResourceCollection {
        todo!()
    }

    // TODO: add validation
    fn use_roadbuild(&mut self, poses: (Edge, Edge), player: PlayerId) {
        todo!()
    }

    // TODO: add validation
    fn use_monopoly(&mut self, resource: Resource, player: PlayerId) {
        todo!()
    }
}
