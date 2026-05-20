use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    algorithm,
    constants::costs,
    gameplay::{
        field::state::{BoardLayout, BoardState, FieldBuildParam},
        primitives::{
            BackAndForthCycle,
            bank::{Bank, BankResourceExchangeError, PlayerResourceExchangeError},
            build::{BoardBuildData, Build, BuildingError, EstablishmentType, Road},
            dev_card::DevCardUsage,
            player::{PlayerDataContainer, PlayerId},
            resource::{Resource, ResourceCollectionError, ResourceSet},
            trade::BankTrade,
            turn::{GameTurn, RegularCycle},
        },
        random::GameRandom,
    },
    topology::Hex,
};

use crate::topology::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableState {
    #[serde(with = "arc_board_layout")]
    pub board: Arc<BoardLayout>,
    pub board_state: BoardState,
    pub bank: Bank,
    pub players: PlayerDataContainer,
    pub builds: BoardBuildData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnedTableState<Cycle = RegularCycle> {
    #[serde(flatten)]
    pub table: TableState,
    pub turn: GameTurn<Cycle>,
}

pub type GameState = TurnedTableState<RegularCycle>;
pub type SetupGameState = TurnedTableState<BackAndForthCycle>;

#[derive(Debug, Default)]
pub struct SetupGameOptions {
    pub random: GameRandom,
}

impl<Cycle> std::ops::Deref for TurnedTableState<Cycle> {
    type Target = TableState;

    fn deref(&self) -> &Self::Target {
        &self.table
    }
}

impl<Cycle> std::ops::DerefMut for TurnedTableState<Cycle> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.table
    }
}

mod arc_board_layout {
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::gameplay::field::state::BoardLayout;

    pub(super) fn serialize<S>(board: &Arc<BoardLayout>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        board.as_ref().serialize(serializer)
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Arc<BoardLayout>, D::Error>
    where
        D: Deserializer<'de>,
    {
        BoardLayout::deserialize(deserializer).map(Arc::new)
    }
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

impl TurnedTableState<RegularCycle> {
    pub fn into_parts(self) -> (TableState, GameTurn) {
        (self.table, self.turn)
    }
}

impl Default for TurnedTableState<BackAndForthCycle> {
    fn default() -> Self {
        Self::new(FieldBuildParam::default())
    }
}

impl TurnedTableState<BackAndForthCycle> {
    pub fn new(field_build_param: FieldBuildParam) -> Self {
        Self::new_with_options(field_build_param, SetupGameOptions::default())
    }

    pub fn new_with_options(
        field_build_param: FieldBuildParam,
        mut options: SetupGameOptions,
    ) -> Self {
        let board = Arc::new(BoardLayout::new(field_build_param));
        let mut bank = Bank::default();
        options.random.shuffle_dev_cards(&mut bank.dev_cards);
        Self::from_board_and_bank(board, bank)
    }

    pub fn new_with_seed(field_build_param: FieldBuildParam, seed: u64) -> Self {
        Self::new_with_options(
            field_build_param,
            SetupGameOptions {
                random: GameRandom::seeded(seed),
            },
        )
    }

    fn from_board_and_bank(board: Arc<BoardLayout>, bank: Bank) -> Self {
        let n_players = board.n_players;
        Self {
            table: TableState {
                players: PlayerDataContainer::new(n_players),
                builds: BoardBuildData::new(n_players),
                board_state: BoardState::new(&board),
                bank,
                board,
            },
            turn: GameTurn::new(n_players as u8),
        }
    }
}

impl TableState {
    pub fn bank_resource_exchange(
        &mut self,
        player_id: PlayerId,
        to_bank: ResourceSet,
        from_bank: ResourceSet,
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
        *self.players.get_mut(player_id).resources() += from_bank;

        Ok(())
    }

    pub fn trade_with_bank(
        &mut self,
        player_id: PlayerId,
        trade: BankTrade,
    ) -> Result<(), BankResourceExchangeError> {
        self.bank_resource_exchange(player_id, trade.to_bank(), trade.from_bank())
    }

    pub fn build(
        &mut self,
        player_id: impl Into<PlayerId>,
        build: Build,
    ) -> Result<(), BuildActionError> {
        let player_id = player_id.into();
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

        let cost = match build {
            Road(_) => costs::ROAD,
            Establishment(establishment) => match establishment.stage {
                Settlement => costs::SETTLEMENT,
                City => costs::CITY,
            },
        };
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

    pub fn buy_dev_card(
        &mut self,
        player_id: impl Into<PlayerId>,
    ) -> Result<crate::gameplay::primitives::dev_card::DevCardKind, BuyDevCardError> {
        let player_id = player_id.into();
        if self.bank.dev_cards.is_empty() {
            return Err(BuyDevCardError::BankIsShort);
        }
        if !self
            .players
            .get(player_id)
            .resources()
            .has_enough(&costs::DEV_CARD)
        {
            return Err(BuyDevCardError::AccountIsShort { id: player_id });
        }

        self.transfer_to_bank(costs::DEV_CARD, player_id)
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

        Ok(card)
    }

    pub fn transfer_to_bank(
        &mut self,
        resources: ResourceSet,
        player_id: impl Into<PlayerId>,
    ) -> Result<(), BankResourceExchangeError> {
        let player_id = player_id.into();
        ResourceSet::transfer(
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
        resources: ResourceSet,
        player_id: impl Into<PlayerId>,
    ) -> Result<(), BankResourceExchangeError> {
        let player_id = player_id.into();
        ResourceSet::transfer(
            &mut self.bank.resources,
            self.players.get_mut(player_id).resources(),
            resources,
        )
        .map_err(|_| BankResourceExchangeError::BankIsShort)
    }

    pub fn players_resource_transfer(
        &mut self,
        from_id: impl Into<PlayerId>,
        to_id: impl Into<PlayerId>,
        resources: ResourceSet,
    ) -> Result<(), PlayerResourceExchangeError> {
        let from_id = from_id.into();
        let to_id = to_id.into();
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

        ResourceSet::transfer(&mut from.resources, &mut to.resources, resources)
            .map_err(|_| PlayerResourceExchangeError::AccountIsShort { id: from_id })
    }

    pub fn players_resource_exchange(
        &mut self,
        lhs: (impl Into<PlayerId>, ResourceSet),
        rhs: (impl Into<PlayerId>, ResourceSet),
    ) -> Result<(), PlayerResourceExchangeError> {
        let lhs = (lhs.0.into(), lhs.1);
        let rhs = (rhs.0.into(), rhs.1);
        let has_enough =
            |(id, rc): &(_, ResourceSet)| self.players.get(*id).resources.has_enough(rc);

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
        algorithm::player_order_from(start_id, self.players.count()).collect::<Vec<_>>()
    }

    pub fn use_robbers(
        &mut self,
        rob_hex: Hex,
        robber_id: PlayerId,
        robbed_id: Option<PlayerId>,
        stolen_resource: Option<Resource>,
    ) -> Result<Option<Resource>, DevCardUsageError> {
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

        self.validate_stolen_resource(robbed_id, stolen_resource)?;
        let stolen = self.transfer_stolen_resource(robbed_id, robber_id, stolen_resource)?;
        self.board_state.robber_pos = rob_hex;
        log::trace!("use robbers success");
        Ok(stolen)
    }

    fn robbery_candidates(&self, rob_hex: Hex, robber_id: PlayerId) -> Vec<PlayerId> {
        algorithm::robbery_candidates(rob_hex, robber_id, &self.builds, &self.players).collect()
    }

    pub fn use_dev_card(
        &mut self,
        usage: DevCardUsage,
        user: impl Into<PlayerId>,
        stolen_resource: Option<Resource>,
    ) -> Result<Option<Resource>, DevCardUsageError> {
        let user = user.into();
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
                self.validate_robbers(*rob_hex, user, *robbed_id)?;
                self.validate_stolen_resource(*robbed_id, stolen_resource)?;
            }
            DevCardUsage::YearOfPlenty(list) => self.validate_year_of_plenty(*list)?,
            DevCardUsage::RoadBuild(poses) => {
                self.validated_roadbuild_state(*poses, user)?;
            }
            DevCardUsage::Monopoly(_) => {
                if stolen_resource.is_some() {
                    return Err(DevCardUsageError::InvalidRobbery);
                }
            }
        }

        if !matches!(usage, DevCardUsage::Knight { .. }) && stolen_resource.is_some() {
            return Err(DevCardUsageError::InvalidRobbery);
        }

        if self
            .players
            .get_mut(user)
            .dev_cards_move_to_used(usage.card_kind())
            .is_err()
        {
            return Err(DevCardUsageError::CardNotFoundInInventory);
        }

        let stolen = match usage {
            DevCardUsage::Knight { rob_hex, robbed_id } => {
                self.use_robbers(rob_hex, user, robbed_id, stolen_resource)?
            }
            DevCardUsage::YearOfPlenty(list) => {
                self.apply_year_of_plenty(list, user)?;
                None
            }
            DevCardUsage::RoadBuild(poses) => {
                self.apply_roadbuild(poses, user)?;
                None
            }
            DevCardUsage::Monopoly(resource) => {
                self.use_monopoly(resource, user)?;
                None
            }
        };

        Ok(stolen)
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

    fn validate_stolen_resource(
        &self,
        robbed_id: Option<PlayerId>,
        stolen_resource: Option<Resource>,
    ) -> Result<(), DevCardUsageError> {
        match (robbed_id, stolen_resource) {
            (Some(id), Some(resource)) => self
                .players
                .get(id)
                .resources()
                .has_enough(&resource.into())
                .then_some(())
                .ok_or(DevCardUsageError::InvalidRobbery),
            (Some(id), None) if self.players.get(id).resources().is_empty() => Ok(()),
            (Some(_), None) => Err(DevCardUsageError::InvalidRobbery),
            (None, None) => Ok(()),
            (None, Some(_)) => Err(DevCardUsageError::InvalidRobbery),
        }
    }

    fn transfer_stolen_resource(
        &mut self,
        robbed_id: Option<PlayerId>,
        robber_id: PlayerId,
        stolen_resource: Option<Resource>,
    ) -> Result<Option<Resource>, DevCardUsageError> {
        log::trace!("steal");
        if let (Some(robbed_id), Some(resource)) = (robbed_id, stolen_resource) {
            self.players_resource_transfer(robbed_id, robber_id, resource.into())
                .map_err(|err| {
                    log::error!("stealing non-existent card: {:?}", err);
                    DevCardUsageError::InvalidRobbery
                })?;
        }
        log::trace!("steal success");
        Ok(stolen_resource)
    }

    fn validate_year_of_plenty(&self, list: [Resource; 2]) -> Result<(), DevCardUsageError> {
        let requested = list
            .into_iter()
            .fold(ResourceSet::default(), |mut acc, resource| {
                acc += resource.into();
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
                .try_build(user, Build::Road(Road { path: pos }))
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
            .player_ids_starting_from(PlayerId::new(0))
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

impl SetupGameState {
    pub fn finish(self) -> GameState {
        GameState {
            table: self.table,
            turn: self.turn.into_regular(),
        }
    }

    pub fn into_setup_parts(self) -> (TableState, GameTurn<BackAndForthCycle>) {
        (self.table, self.turn)
    }
}

#[cfg(test)]
mod tests {
    use super::{DevCardUsageError, GameState};
    use crate::gameplay::field::state::FieldBuildParam;
    use crate::gameplay::game::state::SetupGameState;
    use crate::topology::Hex;
    use crate::{
        gameplay::game::command::InitialPlacementCommand,
        gameplay::primitives::{
            dev_card::{DevCardKind, DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::{Resource, ResourceSet},
        },
    };
    const P0: PlayerId = PlayerId::new(0);
    const P1: PlayerId = PlayerId::new(1);

    fn state_with_two_initial_settlements() -> (GameState, Hex) {
        let mut init = SetupGameState::default();
        let mut victim_hex = None;

        for player_id in 0..2 {
            let (establishment, road) = init
                .builds
                .query()
                .possible_initial_placements(&init.board, player_id)
                .iter()
                .map(InitialPlacementCommand::as_builds)
                .next()
                .expect("default board should have initial placements");

            if player_id == 1 {
                let board_hexes = init.board.arrangement.hex_iter().collect::<Vec<_>>();
                victim_hex =
                    establishment.vtx.as_set().into_iter().find(|hex| {
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

    fn give_active_knight(state: &mut GameState, player_id: impl Into<PlayerId>) {
        let player_id = player_id.into();
        state
            .players
            .get_mut(player_id)
            .dev_cards_add(DevCardKind::Usable(UsableDevCard::Knight));
        state.players.get_mut(player_id).dev_cards_reset_queue();
    }

    #[test]
    fn game_state_serializes_exact_state_for_snapshots() {
        let mut state = SetupGameState::default().finish();
        state.bank.dev_cards = vec![
            DevCardKind::VictoryPoint,
            DevCardKind::Usable(UsableDevCard::Knight),
            DevCardKind::Usable(UsableDevCard::Monopoly),
        ];
        state
            .transfer_from_bank(
                ResourceSet {
                    brick: 3,
                    sheep: 2,
                    ..ResourceSet::EMPTY
                },
                0,
            )
            .unwrap();
        for _ in 0..3 {
            give_active_knight(&mut state, 1);
        }
        for _ in 0..3 {
            state
                .players
                .get_mut(1)
                .dev_cards_move_to_used(UsableDevCard::Knight)
                .unwrap();
        }

        let raw = serde_json::to_string(&state).unwrap();
        let restored: GameState = serde_json::from_str(&raw).unwrap();

        assert!(!raw.contains("\"_p\""));
        assert_eq!(restored.board.n_players, state.board.n_players);
        assert_eq!(
            restored.board.arrangement.len(),
            state.board.arrangement.len()
        );
        assert_eq!(restored.bank.resources, state.bank.resources);
        assert_eq!(restored.bank.dev_cards, state.bank.dev_cards);
        assert_eq!(
            restored.players.get(0).resources(),
            state.players.get(0).resources()
        );
        assert_eq!(restored.players.best_army(), Some(P1));
        assert_eq!(
            restored.players.get(1).dev_cards().used[UsableDevCard::Knight],
            3
        );
    }

    #[test]
    fn game_state_serializes_built_roads_and_settlements_for_snapshots() {
        let (state, _) = state_with_two_initial_settlements();

        let raw = serde_json::to_string(&state).unwrap();
        let restored: GameState = serde_json::from_str(&raw).unwrap();

        assert_eq!(restored.builds.by_player(0).settlements_count(), 1);
        assert_eq!(restored.builds.by_player(0).roads_count(), 1);
        assert_eq!(restored.builds.by_player(1).settlements_count(), 1);
        assert_eq!(restored.builds.by_player(1).roads_count(), 1);
        assert_eq!(
            restored.builds.by_player(0).roads.edges(),
            state.builds.by_player(0).roads.edges()
        );
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
                    robbed_id: Some(P1),
                },
                P0,
                Some(Resource::Brick),
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
            .transfer_from_bank(ResourceSet::from(Resource::Brick), 1)
            .expect("bank should fund test player");

        let err = state
            .use_dev_card(
                DevCardUsage::Knight {
                    rob_hex: victim_hex,
                    robbed_id: None,
                },
                P0,
                None,
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

    #[test]
    fn seeded_initialization_shuffles_dev_cards_deterministically() {
        let first = SetupGameState::new_with_seed(FieldBuildParam::default(), 42);
        let second = SetupGameState::new_with_seed(FieldBuildParam::default(), 42);
        let different = SetupGameState::new_with_seed(FieldBuildParam::default(), 43);

        assert_eq!(first.bank.dev_cards, second.bank.dev_cards);
        assert_ne!(first.bank.dev_cards, different.bank.dev_cards);
    }

    #[test]
    fn initialization_and_game_state_share_table_turn_shape() {
        let init = SetupGameState::default();
        let n_players = init.table.players.count();

        assert_eq!(init.turn.get_turn_index(), PlayerId::new(0));

        let game = init.finish();

        assert_eq!(game.table.players.count(), n_players);
        assert_eq!(game.turn.get_turn_index(), PlayerId::new(0));
    }
}
