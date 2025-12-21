use crate::gameplay::primitives::{
    Robbery,
    bank::{Bank, BankResourceExchangeError, BankView, PlayerResourceExchangeError},
    build::{BuildDataContainer, Builds, Road},
    dev_card::DevCardUsage,
    player::{PlayerDataContainer, PlayerDataProxy, PlayerId, SecuredPlayerData},
    resource::{Resource, ResourceCollection},
    turn::GameTurn,
};

use crate::{gameplay::field::state::FieldState, math::dice::DiceRoller, topology::Path};

#[derive(Debug)]
pub struct GameState {
    pub field: FieldState,
    pub dice: Box<dyn DiceRoller>,
    pub bank: Bank,
    pub turn: GameTurn,
    pub players: PlayerDataContainer,
    pub builds: BuildDataContainer,
}

/// player's perspective on a game, used in `Strategy`
pub struct Perspective<'a> {
    pub player_id: PlayerId,
    pub player_view: PlayerDataProxy<'a>,
    pub field: &'a FieldState,
    pub bank: BankView<'a>,
    pub secured_players_info: Vec<SecuredPlayerData>,
}

impl<'a> Perspective<'a> {
    /// hint: you can call .cycle() on it
    pub fn turn_ids_from_next(&self) -> impl Iterator<Item = PlayerId> {
        let n_players = self.secured_players_info.len() + 1;
        (self.player_id + 1..n_players).chain(0..=self.player_id)
    }
}

#[derive(Debug)]
pub enum DevCardUsageError {
    CardNotFoundInInventory,
    InvalidHex,
    InvalidEdge,
    InvalidRobbery,
    BankIsShort,
}

impl GameState {
    pub fn get_perspective(&self, player_id: PlayerId) -> Perspective {
        let opponents = self
            .players
            .iter()
            .map(|p| SecuredPlayerData::from(&p))
            .collect::<Vec<_>>();

        Perspective {
            player_id,
            player_view: self.players.get(player_id),
            field: &self.field,
            bank: self.bank.view(),
            secured_players_info: opponents,
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
            &mut self.players.get_mut(player_id).resources(),
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
            &mut self.players.get_mut(player_id).resources(),
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
        let (from, to) = self.players.get_mut_both_raw((from_id, to_id));

        ResourceCollection::transfer(
            &mut from.resources,
            &mut to.resources,
            resources,
            PlayerResourceExchangeError::AccountIsShort { id: from_id },
        )
    }

    pub fn players_resource_exchange(
        &mut self,
        lhs: (PlayerId, ResourceCollection),
        rhs: (PlayerId, ResourceCollection),
    ) -> Result<(), PlayerResourceExchangeError> {
        let has_enough =
            |(id, rc): &(_, ResourceCollection)| self.players.get(*id).resources.has_enough(rc);

        match (has_enough(&lhs), has_enough(&rhs)) {
            (false, _) => Err(PlayerResourceExchangeError::AccountIsShort { id: lhs.0 }),
            (_, false) => Err(PlayerResourceExchangeError::AccountIsShort { id: rhs.0 }),
            _ => {
                self.players_resource_transfer(lhs.0, rhs.0, lhs.1)?;
                self.players_resource_transfer(lhs.0, rhs.0, lhs.1)
            }
        }
    }

    pub fn player_ids_starting_from(
        &self,
        start_id: PlayerId,
    ) -> impl IntoIterator<Item = PlayerId> + use<> {
        (start_id..self.players.count())
            .chain(0..start_id)
            .collect::<Vec<_>>()
    }

    pub fn count_max_tract_length(&self, player_id: PlayerId) -> u16 {
        self.builds[player_id].roads.calculate_diameter() as u16
    }

    /// goes through the players and if one have >9 vp returns it
    pub fn check_win_condition(&self) -> Option<PlayerId> {
        const VP_TO_WIN: u8 = 10; // TODO: move outside to config
        for player_id in self.player_ids_starting_from(0) {
            let pure_vp = self.count_vp_without_track_and_army(player_id);

            let tract_pts = match self.builds.longest_road() {
                Some(id) if id == player_id => 2,
                _ => 0,
            };
            let army_pts = if self.players.get(player_id).has_largest_army() {
                3
            } else {
                0
            };
            if pure_vp + tract_pts + army_pts > VP_TO_WIN as u16 {
                return Some(player_id);
            }
        }

        None
    }

    pub fn count_vp_without_track_and_army(&self, player_id: PlayerId) -> u16 {
        let mut score = 0;
        score += self.players.get(player_id).dev_cards().victory_pts;
        score += self.builds[player_id].settlements.len() as u16;
        score += self.builds[player_id].cities.len() as u16 * 2;
        score
    }

    /// no, not in that way
    pub fn use_robbers(
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
            match self.builds.builds_on_hex(rob_request.hex).get(&robbed_id) {
                Some(v) if !v.settlements.is_empty() || !v.cities.is_empty() => {
                    self.steal(robbed_id, robber_id)
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
                self.use_robbers(rob_request, user)?;
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

        if let Err(_) = self
            .players
            .get_mut(user)
            .dev_cards_move_to_used(usage.card_kind())
        {
            return Err(DevCardUsageError::CardNotFoundInInventory);
        }

        Ok(())
    }

    /* private helper functions */

    /// steal random card from another player
    fn steal(&mut self, robbed_id: PlayerId, robber_id: PlayerId) {
        let robbed_account = self.players.get(robbed_id).resources();
        let stolen = robbed_account.peek_random();
        if let Some(card) = stolen {
            if let Err(e) = self.players_resource_transfer(robbed_id, robber_id, card.into()) {
                log::error!("stealing non-existent card: {:?}", e)
            }
        }
    }

    fn use_year_of_plenty(
        &mut self,
        list: [Resource; 2],
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        for resource in list {
            if let Err(_) = self.transfer_from_bank(resource.into(), user) {
                return Err(DevCardUsageError::BankIsShort);
            }
        }

        Ok(())
    }

    fn use_roadbuild(&mut self, poses: [Path; 2], user: PlayerId) -> Result<(), DevCardUsageError> {
        for pos in poses {
            if let Err(err) = self.builds.try_build(user, Builds::Road(Road { pos })) {
                log::info!("invalid placement try: {:?}", err);
                return Err(DevCardUsageError::InvalidEdge);
            }
        }

        Ok(())
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
            let resources = (resource, self.players.get(id).resources()[resource]).into();
            if let Err(e) = self.players_resource_transfer(id, user, resources) {
                log::error!("somehow took more cards than a player has: {:?}", e);
            }
        }

        Ok(())
    }
}
