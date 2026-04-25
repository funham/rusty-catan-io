use serde::{Deserialize, Serialize};

use crate::{
    gameplay::{
        field::state::BuildCollection,
        game::view::{GameSnapshot, GameView, PrivatePlayerData, PublicPlayerState},
        primitives::{
            bank::{Bank, BankResourceExchangeError, BankViewOwned, PlayerResourceExchangeError},
            build::{BoardBuildData, Build, Road},
            dev_card::DevCardUsage,
            player::{PlayerData, PlayerDataContainer, PlayerId, SecuredPlayerData},
            resource::{Resource, ResourceCollection},
            turn::GameTurn,
        },
    },
    topology::Hex,
};

use crate::{gameplay::field::state::FieldState, topology::Path};

#[derive(Debug)]
pub struct GameState {
    pub field: FieldState,
    pub turn: GameTurn,
    pub bank: Bank,
    pub players: PlayerDataContainer,
    pub builds: BoardBuildData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisiblePlayer {
    pub player_id: PlayerId,
    pub public_data: SecuredPlayerData,
    pub builds: BuildCollection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perspective {
    pub player_id: PlayerId,
    pub player_view: PlayerData,
    pub field: FieldState,
    pub bank: BankViewOwned,
    pub builds: BoardBuildData,
    // TODO: change to just visible_players so that there'd be one source of information for how player's occuring to the opponents
    pub other_players: Vec<VisiblePlayer>,
}

impl Perspective {
    pub fn turn_ids_from_next(&self) -> impl Iterator<Item = PlayerId> {
        let n_players = self.other_players.len() + 1;
        (self.player_id + 1..n_players).chain(0..self.player_id)
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
    pub fn snapshot(&self) -> GameSnapshot {
        GameSnapshot {
            current_player_id: self.turn.get_turn_index(),
            rounds_played: self.turn.get_rounds_played(),
            field: self.field.clone(),
            bank: self.bank.public_view(),
            players: self
                .players
                .iter()
                .enumerate()
                .map(|(player_id, player)| PublicPlayerState {
                    player_id,
                    public_data: SecuredPlayerData::from(&player),
                    builds: self.builds.query().all_builds()[player_id].clone(),
                })
                .collect(),
            longest_road_owner: self.builds.longest_road(),
            largest_army_owner: self.players.best_army(),
        }
    }

    pub fn view(&self) -> GameView {
        todo!()
    }

    pub fn private_view(&self, _player_id: PlayerId) -> PrivatePlayerData {
        todo!()
    }

    pub fn bank_resource_exchange(
        &mut self,
        player_id: PlayerId,
        to_bank: ResourceCollection,
        from_bank: ResourceCollection,
    ) -> Result<(), BankResourceExchangeError> {
        self.transfer_to_bank(to_bank, player_id)?;
        if !self.bank.resources.has_enough(&from_bank) {
            return Err(BankResourceExchangeError::BankIsShort);
        }
        self.transfer_from_bank(from_bank, player_id)
    }

    pub fn transfer_to_bank(
        &mut self,
        resources: ResourceCollection,
        player_id: PlayerId,
    ) -> Result<(), BankResourceExchangeError> {
        ResourceCollection::transfer(
            self.players.get_mut(player_id).resources(),
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
            self.players.get_mut(player_id).resources(),
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
        log::trace!("players_resource_transfer");
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
                self.players_resource_transfer(rhs.0, lhs.0, rhs.1)
            }
        }
    }

    pub fn player_ids_starting_from(&self, start_id: PlayerId) -> Vec<PlayerId> {
        (start_id..self.players.count())
            .chain(0..start_id)
            .collect::<Vec<_>>()
    }

    pub fn is_player_on_hex(&self, player_id: PlayerId, hex: Hex) -> bool {
        for v in hex.vertices() {
            let has_build_on_intersection = self.builds[player_id]
                .establishments
                .iter()
                .any(|est| est.pos == v);
            if has_build_on_intersection {
                return true;
            }
        }

        false
    }

    pub fn players_on_hex(&self, hex: Hex) -> Vec<PlayerId> {
        self.player_ids_starting_from(0)
            .into_iter()
            .filter(|id| self.is_player_on_hex(*id, hex))
            .collect()
    }

    pub fn count_max_tract_length(&self, player_id: PlayerId) -> u16 {
        self.builds[player_id].roads.find_longest_trail_length() as u16
    }

    pub fn check_win_condition(&self) -> Option<PlayerId> {
        const VP_TO_WIN: u8 = 10;
        for player_id in self.player_ids_starting_from(0) {
            let pure_vp = self.count_dev_card_build_vp(player_id);

            let tract_pts = match self.builds.longest_road() {
                Some(id) if id == player_id => 2,
                _ => 0,
            };
            let army_pts = if self.players.get(player_id).has_largest_army() {
                3
            } else {
                0
            };
            if pure_vp + tract_pts + army_pts >= VP_TO_WIN as u16 {
                return Some(player_id);
            }
        }

        None
    }

    pub fn count_dev_card_build_vp(&self, player_id: PlayerId) -> u16 {
        let mut score = 0;

        // dev card victory points
        score += self.players.get(player_id).dev_cards().victory_pts;

        // build victory points
        score += self.builds[player_id]
            .establishments
            .iter()
            .map(|est| match est.stage {
                crate::gameplay::primitives::build::EstablishmentType::Settlement => 1,
                crate::gameplay::primitives::build::EstablishmentType::City => 2,
            })
            .sum::<u16>();

        score
    }

    pub fn use_robbers(
        &mut self,
        rob_hex: Hex,
        robber_id: PlayerId,
        robbed_id: Option<PlayerId>,
    ) -> Result<(), DevCardUsageError> {
        log::trace!("use robbers");

        if (self.field.arrangement.field_radius as usize) < rob_hex.norm() {
            return Err(DevCardUsageError::InvalidHex);
        }

        self.field.robber_pos = rob_hex;

        if let Some(robbed_id) = robbed_id {
            match self.builds.query().builds_on_hex(rob_hex).get(&robbed_id) {
                Some(v) if !v.establishments.is_empty() => self.steal(robbed_id, robber_id),
                _ => {
                    log::trace!("use robbers fail");
                    return Err(DevCardUsageError::InvalidRobbery);
                }
            }
        }
        log::trace!("use robbers success");
        Ok(())
    }

    pub fn use_dev_card(
        &mut self,
        usage: DevCardUsage,
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        match usage {
            DevCardUsage::Knight(_rob_hex) => {
                return Err(DevCardUsageError::InvalidRobbery);
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

        if self
            .players
            .get_mut(user)
            .dev_cards_move_to_used(usage.card_kind())
            .is_err()
        {
            return Err(DevCardUsageError::CardNotFoundInInventory);
        }

        Ok(())
    }

    fn steal(&mut self, robbed_id: PlayerId, robber_id: PlayerId) {
        log::trace!("steal");
        let robbed_account = self.players.get(robbed_id).resources();
        let stolen = robbed_account.peek_random();
        log::trace!("peek random success");
        if let Some(card) = stolen {
            if let Err(e) = self.players_resource_transfer(robbed_id, robber_id, card.into()) {
                log::error!("stealing non-existent card: {:?}", e)
            }
        }
        log::trace!("steal success");
    }

    fn use_year_of_plenty(
        &mut self,
        list: [Resource; 2],
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        for resource in list {
            if self.transfer_from_bank(resource.into(), user).is_err() {
                return Err(DevCardUsageError::BankIsShort);
            }
        }

        Ok(())
    }

    fn use_roadbuild(&mut self, poses: [Path; 2], user: PlayerId) -> Result<(), DevCardUsageError> {
        for pos in poses {
            if let Err(err) = self.builds.try_build(user, Build::Road(Road { pos })) {
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
