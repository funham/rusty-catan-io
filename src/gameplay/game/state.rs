use std::collections::BTreeMap;

use crate::gameplay::dev_card::UsableDevCardKind;
use crate::{
    gameplay::{
        bank::{Bank, BankResourceExchangeError, BankView, PlayerResourceExchangeError},
        field::state::Field,
        player::{OpponentData, PlayerData, PlayerId},
        primitives::*,
        resource::{Resource, ResourceCollection},
        turn::GameTurn,
    },
    math::dice::DiceRoller,
    topology::Path,
};

#[derive(Debug)]
pub struct GameState {
    pub(super) field: Field,
    pub(super) dice: Box<dyn DiceRoller>,
    pub(super) bank: Bank,
    pub(super) players: Vec<PlayerData>,
    pub(super) turn: GameTurn,
    pub(super) longest_tract: Option<PlayerId>,
    pub(super) largest_army: Option<PlayerId>,
}

/// player's perspective on a game, used in `Strategy`
pub struct Perspective<'a> {
    pub player_id: PlayerId,
    pub player_data: &'a PlayerData,
    pub field: &'a Field,
    pub bank: BankView<'a>,
    pub opponents: BTreeMap<PlayerId, OpponentData>,
}

impl<'a> Perspective<'a> {
    /// hint: you can call .cycle() on it
    pub fn turn_ids_from_next(&self) -> impl Iterator<Item = PlayerId> {
        let n_players = self.opponents.len() + 1;
        (self.player_id + 1..n_players).chain(0..=self.player_id)
    }
}

#[derive(Debug)]
pub enum DevCardUsageError {
    CardNotFoundInInventory,
    InvalidHex,
    InvalidEdge,
    InvalidRobbery,
}

impl GameState {
    pub fn get_perspective(&self, player_id: PlayerId) -> Perspective {
        let opponents = self
            .players
            .iter()
            .enumerate()
            .filter(|(i, _)| i != &player_id)
            .map(|(i, p)| (i, OpponentData::from(p)))
            .collect::<BTreeMap<PlayerId, OpponentData>>();

        Perspective {
            player_id,
            player_data: &self.players[player_id],
            field: &self.field,
            bank: self.bank.view(),
            opponents,
        }
    }

    pub fn bank_resource_exchange(
        &mut self,
        player_id: PlayerId,
        to_bank: ResourceCollection,
        from_bank: ResourceCollection,
    ) -> Result<(), BankResourceExchangeError> {
        self.transfer_to_bank(to_bank, player_id)?;
        self.transfer_from_bank(from_bank, player_id)
    }

    pub fn transfer_to_bank(
        &mut self,
        resources: ResourceCollection,
        player_id: PlayerId,
    ) -> Result<(), BankResourceExchangeError> {
        ResourceCollection::transfer(
            &mut self.players[player_id].resources,
            &mut self.bank.resources,
            resources,
            BankResourceExchangeError::AccountIsShort { id: player_id },
        )
    }

    pub fn transfer_from_bank(
        &mut self,
        resources: ResourceCollection,
        player_id: PlayerId,
    ) -> Result<(), BankResourceExchangeError> {
        ResourceCollection::transfer(
            &mut self.bank.resources,
            &mut self.players[player_id].resources,
            resources,
            BankResourceExchangeError::BankIsShort,
        )
    }

    pub fn players_resource_transfer(
        &mut self,
        from_id: PlayerId,
        to_id: PlayerId,
        resources: ResourceCollection,
    ) -> Result<(), PlayerResourceExchangeError> {
        let (from, to) = self.get_mut_players(from_id, to_id);
        ResourceCollection::transfer(
            &mut from.resources,
            &mut to.resources,
            resources,
            PlayerResourceExchangeError::AccountIsShort { id: from_id },
        )
    }

    pub fn player_ids_starting_from(
        &self,
        start_id: PlayerId,
    ) -> impl IntoIterator<Item = PlayerId> + use<> {
        (start_id..self.players.len())
            .chain(0..start_id)
            .collect::<Vec<_>>()
    }

    pub fn count_max_tract_length(&self, player_id: PlayerId) -> u16 {
        todo!(
            "implement some graph algorithm (maybe store graphs for each player in `PlayerBuildData`"
        )
    }

    /// goes through the players and if one have >9 vp returns it
    pub fn check_win_condition(&self) -> Option<PlayerId> {
        const VP_TO_WIN: u8 = 10; // TODO: move outside to config
        for player_id in self.player_ids_starting_from(0) {
            let pure_vp = self.count_vp_without_track_and_army(player_id);
            let tract_pts = match self.longest_tract {
                Some(id) if id == player_id => 2,
                _ => 0,
            };
            let army_pts = match self.largest_army {
                Some(id) if id == player_id => 2,
                _ => 0,
            };

            if pure_vp + tract_pts + army_pts > VP_TO_WIN as u16 {
                return Some(player_id);
            }
        }

        None
    }

    pub fn count_vp_without_track_and_army(&self, player_id: PlayerId) -> u16 {
        let mut score = 0;
        score += self.players[player_id].dev_cards.victory_pts;
        score += self.field.builds[player_id].settlements.len() as u16;
        score += self.field.builds[player_id].cities.len() as u16 * 2;
        score
    }

    /// no, not in that way
    pub fn execute_robbers(
        &mut self,
        rob_request: Robbery,
        robber_id: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        // move robbers
        if (self.field.field_radius as i32) < rob_request.hex.len() {
            return Err(DevCardUsageError::InvalidHex);
        }

        self.field.robber_pos = rob_request.hex;

        // steal card
        if let Some(robbed_id) = rob_request.robbed {
            match self.field.builds_on_hex(rob_request.hex).get(&robbed_id) {
                Some(v) if !v.settlements.is_empty() || !v.cities.is_empty() => {
                    self.rob(robbed_id, robber_id)
                }
                _ => return Err(DevCardUsageError::InvalidRobbery),
            }
        }
        Ok(())
    }

    pub fn use_dev_card(
        &mut self,
        usage: DevCardUsage,
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        match usage {
            DevCardUsage::Knight(rob_request) => {
                self.use_knight(rob_request, user)?;
            }
            DevCardUsage::YearOfPlenty(list) => {
                self.use_year_of_plenty(list, user)?;
            }
            DevCardUsage::RoadBuild(x) => {
                self.use_roadbuild(x, user)?;
            }
            DevCardUsage::Monopoly(resource) => {
                self.use_monopoly(resource, user)?;
            }
        }

        if let Err(_) = self.players[user].dev_cards.move_to_played(usage.card()) {
            return Err(DevCardUsageError::CardNotFoundInInventory);
        }

        Ok(())
    }

    /* private helper functions */

    // get mutable view of two players
    fn get_mut_players(
        &mut self,
        player1: PlayerId,
        player2: PlayerId,
    ) -> (&mut PlayerData, &mut PlayerData) {
        let (left, right) = self.players.split_at_mut(player1.max(player2));
        let left_len = left.len();
        let ((half1, p1), (half2, p2)) = match player1.cmp(&player2) {
            std::cmp::Ordering::Equal => unreachable!("you can't rob yourself"),
            std::cmp::Ordering::Less => ((left, player1), (right, player2 - left_len)),
            std::cmp::Ordering::Greater => ((right, player1 - left_len), (left, player2)),
        };

        (&mut half1[p1], &mut half2[p2])
    }

    /// steal random card from another player
    fn rob(&mut self, robbed_id: PlayerId, robber_id: PlayerId) {
        let robbed_account = &self.players[robbed_id].resources;
        let stolen = robbed_account.peek_random();
        if let Some(card) = stolen {
            if let Err(e) = self.players_resource_transfer(robbed_id, robber_id, card.into()) {
                log::error!("stealing non-existent card: {:?}", e)
            }
        }
    }

    fn use_knight(
        &mut self,
        rob_request: Robbery,
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        self.execute_robbers(rob_request, user)?;

        // update largest army logic
        let knight_count = self.players[user].dev_cards.played[UsableDevCardKind::Knight] + 1;

        let curr_best_count = match self.largest_army {
            Some(id) => self.players[id].dev_cards.played[UsableDevCardKind::Knight],
            None => 2, // a bit dangerous hack
        };

        if knight_count > curr_best_count {
            self.largest_army = Some(user);
        }

        Ok(())
    }

    fn use_year_of_plenty(
        &mut self,
        list: [Resource; 2],
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        todo!()
    }

    fn use_roadbuild(&mut self, poses: [Path; 2], user: PlayerId) -> Result<(), DevCardUsageError> {
        todo!()
    }

    fn use_monopoly(
        &mut self,
        resource: Resource,
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        for id in self
            .player_ids_starting_from(0)
            .into_iter()
            .filter(|id| *id != user)
        {
            let resources = (resource, self.players[id].resources[resource]).into();
            if let Err(e) = self.players_resource_transfer(id, user, resources) {
                log::error!("somehow took more cards than a player has: {:?}", e);
            }
        }

        Ok(())
    }
}
