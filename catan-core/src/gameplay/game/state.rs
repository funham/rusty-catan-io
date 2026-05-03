use std::sync::Arc;

use crate::{
    gameplay::{
        field::state::{BoardLayout, BoardState},
        primitives::{
            bank::{Bank, BankResourceExchangeError, PlayerResourceExchangeError},
            build::{BoardBuildData, Build, BuildingError, EstablishmentType, Road},
            dev_card::DevCardUsage,
            player::{PlayerDataContainer, PlayerId},
            resource::{HasCost, Resource, ResourceCollection, ResourceCollectionError},
            trade::BankTrade,
            turn::GameTurn,
        },
    },
    topology::Hex,
};

use crate::topology::Path;

#[derive(Debug, Clone)]
pub struct GameState {
    pub board: Arc<BoardLayout>,
    pub board_state: BoardState,
    pub turn: GameTurn,
    pub bank: Bank,
    pub players: PlayerDataContainer,
    pub builds: BoardBuildData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevCardUsageError {
    CardNotFoundInInventory,
    InvalidHex,
    InvalidEdge,
    InvalidRobbery,
    BankIsShort,
}

#[derive(Debug)]
pub enum BuildActionError {
    AccountIsShort { id: PlayerId },
    OutOfPieces,
    InvalidPlacement(BuildingError),
}

#[derive(Debug)]
pub enum BuyDevCardError {
    AccountIsShort { id: PlayerId },
    BankIsShort,
}

impl GameState {
    pub fn bank_resource_exchange(
        &mut self,
        player_id: PlayerId,
        to_bank: ResourceCollection,
        from_bank: ResourceCollection,
    ) -> Result<(), BankResourceExchangeError> {
        let missing = self.players.get(player_id).resources().missing(&to_bank);
        if !missing.is_empty() {
            return Err(BankResourceExchangeError::AccountIsShort {
                account: player_id,
                short: missing,
            });
        }
        if !self.bank.can_pay(&from_bank) {
            return Err(BankResourceExchangeError::BankIsShort);
        }

        self.players
            .get_mut(player_id)
            .resources()
            .subtract_in_place(&to_bank)
            .map_err(|_| BankResourceExchangeError::AccountIsShort {
                account: player_id,
                short: self
                    .players
                    .get_mut(player_id)
                    .resources()
                    .missing(&to_bank),
            })?;
        self.bank.deposit(to_bank);
        self.bank.withdraw(from_bank)?;
        *self.players.get_mut(player_id).resources() += &from_bank;

        Ok(())
    }

    pub fn trade_with_bank(
        &mut self,
        player_id: PlayerId,
        trade: BankTrade,
    ) -> Result<(), BankResourceExchangeError> {
        self.bank_resource_exchange(player_id, trade.to_bank(), trade.from_bank())
    }

    pub fn build(&mut self, player_id: PlayerId, build: Build) -> Result<(), BuildActionError> {
        use Build::*;
        use EstablishmentType::*;

        let out_of_pieces = match build {
            Establishment(establishment) => match establishment.stage {
                Settlement => self.builds.by_player(player_id).settlements_count() >= 5,
                City => self.builds.by_player(player_id).cities_count() >= 5,
            },
            Road(_) => self.builds.by_player(player_id).roads_count() >= 15,
        };

        if let Some(err) = out_of_pieces.then_some(BuildActionError::OutOfPieces) {
            return Err(err);
        }

        let cost = build.cost();
        if !self.players.get(player_id).resources().has_enough(&cost) {
            return Err(BuildActionError::AccountIsShort { id: player_id });
        }

        let mut builds = self.builds.clone();
        builds
            .try_build(player_id, build)
            .map_err(BuildActionError::InvalidPlacement)?;

        self.transfer_to_bank(cost, player_id)
            .map_err(|err| match err {
                BankResourceExchangeError::BankIsShort => unreachable!(),
                BankResourceExchangeError::AccountIsShort {
                    account: id,
                    short: _,
                } => BuildActionError::AccountIsShort { id },
            })?;
        self.builds = builds;

        Ok(())
    }

    pub fn buy_dev_card(&mut self, player_id: PlayerId) -> Result<(), BuyDevCardError> {
        const COST: ResourceCollection = ResourceCollection {
            wheat: 1,
            sheep: 1,
            ore: 1,
            ..ResourceCollection::ZERO
        };

        if self.bank.dev_cards.is_empty() {
            return Err(BuyDevCardError::BankIsShort);
        }
        if !self.players.get(player_id).resources().has_enough(&COST) {
            return Err(BuyDevCardError::AccountIsShort { id: player_id });
        }

        self.transfer_to_bank(COST, player_id)
            .map_err(|err| match err {
                BankResourceExchangeError::BankIsShort => unreachable!(),
                BankResourceExchangeError::AccountIsShort {
                    account: id,
                    short: _,
                } => BuyDevCardError::AccountIsShort { id },
            })?;

        let card = self
            .bank
            .draw_dev_card()
            .ok_or(BuyDevCardError::BankIsShort)?;
        self.players.get_mut(player_id).dev_cards_add(card);

        Ok(())
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
        )
        .map_err(|err| BankResourceExchangeError::AccountIsShort {
            account: player_id,
            short: match err {
                ResourceCollectionError::InsufficientResources {
                    available,
                    required,
                } => available.missing(&required),
                ResourceCollectionError::ResourceAppearsTwice => unreachable!(),
            },
        })
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
        )
        .map_err(|_| BankResourceExchangeError::BankIsShort)
    }

    pub fn players_resource_transfer(
        &mut self,
        from_id: PlayerId,
        to_id: PlayerId,
        resources: ResourceCollection,
    ) -> Result<(), PlayerResourceExchangeError> {
        log::trace!("players_resource_transfer");
        if from_id == to_id {
            return self
                .players
                .get(from_id)
                .resources()
                .has_enough(&resources)
                .then_some(())
                .ok_or(PlayerResourceExchangeError::AccountIsShort { id: from_id });
        }

        let (from, to) = self.players.get_mut_both_raw((from_id, to_id));

        ResourceCollection::transfer(&mut from.resources, &mut to.resources, resources)
            .map_err(|_| PlayerResourceExchangeError::AccountIsShort { id: from_id })
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

    fn player_ids_starting_from(&self, start_id: PlayerId) -> Vec<PlayerId> {
        (start_id..self.players.count())
            .chain(0..start_id)
            .collect::<Vec<_>>()
    }

    pub fn use_robbers(
        &mut self,
        rob_hex: Hex,
        robber_id: PlayerId,
        robbed_id: Option<PlayerId>,
    ) -> Result<(), DevCardUsageError> {
        log::trace!("use robbers");

        if (self.board.arrangement.radius() as usize) < rob_hex.norm() {
            return Err(DevCardUsageError::InvalidHex);
        }

        if rob_hex == self.board_state.robber_pos {
            return Err(DevCardUsageError::InvalidRobbery);
        }

        let candidates = self.robbery_candidates(rob_hex, robber_id);
        if let Some(robbed_id) = robbed_id {
            if !candidates.contains(&robbed_id) {
                log::trace!("use robbers fail");
                return Err(DevCardUsageError::InvalidRobbery);
            }
        } else if !candidates.is_empty() {
            return Err(DevCardUsageError::InvalidRobbery);
        }

        self.board_state.robber_pos = rob_hex;
        if let Some(robbed_id) = robbed_id {
            self.steal(robbed_id, robber_id);
        }
        log::trace!("use robbers success");
        Ok(())
    }

    fn robbery_candidates(&self, rob_hex: Hex, robber_id: PlayerId) -> Vec<PlayerId> {
        self.builds
            .query()
            .builds_on_hex(rob_hex)
            .into_iter()
            .filter(|(id, builds)| {
                *id != robber_id
                    && !builds.establishments.is_empty()
                    && !self.players.get(*id).resources().is_empty()
            })
            .map(|(id, _)| id)
            .collect()
    }

    pub fn use_dev_card(
        &mut self,
        usage: DevCardUsage,
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        if !self
            .players
            .get(user)
            .dev_cards()
            .active
            .contains(usage.card_kind())
        {
            return Err(DevCardUsageError::CardNotFoundInInventory);
        }

        match &usage {
            DevCardUsage::Knight { rob_hex, robbed_id } => {
                self.validate_robbers(*rob_hex, user, *robbed_id)?
            }
            DevCardUsage::YearOfPlenty(list) => self.validate_year_of_plenty(*list)?,
            DevCardUsage::RoadBuild(poses) => {
                self.validated_roadbuild_state(*poses, user)?;
            }
            DevCardUsage::Monopoly(_) => {}
        }

        if self
            .players
            .get_mut(user)
            .dev_cards_move_to_used(usage.card_kind())
            .is_err()
        {
            return Err(DevCardUsageError::CardNotFoundInInventory);
        }

        match usage {
            DevCardUsage::Knight { rob_hex, robbed_id } => {
                self.use_robbers(rob_hex, user, robbed_id)?
            }
            DevCardUsage::YearOfPlenty(list) => self.apply_year_of_plenty(list, user)?,
            DevCardUsage::RoadBuild(poses) => self.apply_roadbuild(poses, user)?,
            DevCardUsage::Monopoly(resource) => self.use_monopoly(resource, user)?,
        }

        Ok(())
    }

    fn validate_robbers(
        &self,
        rob_hex: Hex,
        robber_id: PlayerId,
        robbed_id: Option<PlayerId>,
    ) -> Result<(), DevCardUsageError> {
        if (self.board.arrangement.radius() as usize) < rob_hex.norm() {
            return Err(DevCardUsageError::InvalidHex);
        }
        if rob_hex == self.board_state.robber_pos {
            return Err(DevCardUsageError::InvalidRobbery);
        }

        let candidates = self.robbery_candidates(rob_hex, robber_id);
        match robbed_id {
            Some(id) if candidates.contains(&id) => Ok(()),
            Some(_) => Err(DevCardUsageError::InvalidRobbery),
            None if candidates.is_empty() => Ok(()),
            None => Err(DevCardUsageError::InvalidRobbery),
        }
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

    fn validate_year_of_plenty(&self, list: [Resource; 2]) -> Result<(), DevCardUsageError> {
        let requested =
            list.into_iter()
                .fold(ResourceCollection::default(), |mut acc, resource| {
                    acc += &resource.into();
                    acc
                });

        self.bank
            .can_pay(&requested)
            .then_some(())
            .ok_or(DevCardUsageError::BankIsShort)
    }

    fn apply_year_of_plenty(
        &mut self,
        list: [Resource; 2],
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        self.validate_year_of_plenty(list)?;

        for resource in list {
            self.transfer_from_bank(resource.into(), user)
                .map_err(|_| DevCardUsageError::BankIsShort)?;
        }

        Ok(())
    }

    fn validated_roadbuild_state(
        &self,
        poses: [Path; 2],
        user: PlayerId,
    ) -> Result<BoardBuildData, DevCardUsageError> {
        let mut builds = self.builds.clone();
        for pos in poses {
            builds
                .try_build(user, Build::Road(Road { pos }))
                .map_err(|_| DevCardUsageError::InvalidEdge)?;
        }

        Ok(builds)
    }

    fn apply_roadbuild(
        &mut self,
        poses: [Path; 2],
        user: PlayerId,
    ) -> Result<(), DevCardUsageError> {
        self.builds = self.validated_roadbuild_state(poses, user)?;

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

#[cfg(test)]
mod tests {
    use super::{DevCardUsageError, GameState};
    use crate::gameplay::{
        game::init::GameInitializationState,
        primitives::{
            dev_card::{DevCardKind, DevCardUsage, UsableDevCard},
            resource::{Resource, ResourceCollection},
        },
    };
    use crate::topology::Hex;

    fn state_with_two_initial_settlements() -> (GameState, Hex) {
        let mut init = GameInitializationState::default();
        let mut victim_hex = None;

        for player_id in 0..2 {
            let (establishment, road) = init
                .builds
                .query()
                .possible_initial_placements(&init.board, player_id)
                .into_iter()
                .next()
                .expect("default board should have initial placements");

            if player_id == 1 {
                let board_hexes = init.board.arrangement.hex_iter().collect::<Vec<_>>();
                victim_hex =
                    establishment.pos.as_set().into_iter().find(|hex| {
                        *hex != init.board_state.robber_pos && board_hexes.contains(hex)
                    });
            }

            init.builds
                .try_init_place(player_id, road, establishment)
                .expect("generated initial placement should be valid");
        }

        (
            init.finish(),
            victim_hex.expect("victim settlement should touch a non-robber hex"),
        )
    }

    fn give_active_knight(state: &mut GameState, player_id: usize) {
        state
            .players
            .get_mut(player_id)
            .dev_cards_add(DevCardKind::Usable(UsableDevCard::Knight));
        state.players.get_mut(player_id).dev_cards_reset_queue();
    }

    #[test]
    fn knight_moves_robber_steals_and_marks_card_used() {
        let (mut state, victim_hex) = state_with_two_initial_settlements();
        give_active_knight(&mut state, 0);
        state
            .transfer_from_bank(Resource::Brick.into(), 1)
            .expect("bank should fund test player");

        state
            .use_dev_card(
                DevCardUsage::Knight {
                    rob_hex: victim_hex,
                    robbed_id: Some(1),
                },
                0,
            )
            .expect("knight usage should be legal");

        assert_eq!(state.board_state.robber_pos, victim_hex);
        assert_eq!(state.players.get(0).resources().total(), 1);
        assert_eq!(state.players.get(1).resources().total(), 0);
        assert_eq!(
            state.players.get(0).dev_cards().used[UsableDevCard::Knight],
            1
        );
        assert_eq!(
            state.players.get(0).dev_cards().active[UsableDevCard::Knight],
            0
        );
    }

    #[test]
    fn knight_requires_target_when_robbable_player_exists() {
        let (mut state, victim_hex) = state_with_two_initial_settlements();
        let initial_robber = state.board_state.robber_pos;
        give_active_knight(&mut state, 0);
        state
            .transfer_from_bank(ResourceCollection::from(Resource::Brick), 1)
            .expect("bank should fund test player");

        let err = state
            .use_dev_card(
                DevCardUsage::Knight {
                    rob_hex: victim_hex,
                    robbed_id: None,
                },
                0,
            )
            .expect_err("target must be provided when a player can be robbed");

        assert_eq!(err, DevCardUsageError::InvalidRobbery);
        assert_eq!(state.board_state.robber_pos, initial_robber);
        assert_eq!(
            state.players.get(0).dev_cards().active[UsableDevCard::Knight],
            1
        );
        assert_eq!(
            state.players.get(0).dev_cards().used[UsableDevCard::Knight],
            0
        );
    }
}
