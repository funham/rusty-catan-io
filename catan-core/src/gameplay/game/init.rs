use crate::gameplay::{
    field::state::{FieldBuildParam, FieldState},
    game::state::{GameState, Perspective, VisiblePlayer},
    primitives::{
        bank::Bank,
        build::BoardBuildData,
        player::{PlayerDataContainer, PlayerId, SecuredPlayerData},
        turn::{BackAndForthCycle, GameTurn},
    },
};

pub struct GameInitializationState {
    pub field: FieldState,
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
        let field = FieldState::new(field_build_param);
        Self {
            turn: GameTurn::new(field.n_players as u8),
            players: PlayerDataContainer::new(field.n_players),
            builds: BoardBuildData::new(field.n_players),
            field,
            bank: Default::default(),
        }
    }

    pub fn perspective(&self, player_id: PlayerId) -> Perspective {
        let other_players = self
            .players
            .iter()
            .enumerate()
            .filter(|(id, _)| *id != player_id)
            .map(|(id, player)| VisiblePlayer {
                player_id: id,
                public_data: SecuredPlayerData::from(&player),
                builds: self.builds.query().all_builds()[id].clone(),
            })
            .collect();

        Perspective {
            player_id,
            player_view: self
                .players
                .get(player_id)
                .resources()
                .clone()
                .into_player_data(self.players.get(player_id).dev_cards().clone()),
            field: self.field.clone(),
            bank: self.bank.public_view(),
            builds: self.builds.clone(),
            other_players,
        }
    }

    pub fn finish(self) -> GameState {
        GameState {
            field: self.field,
            turn: self.turn.into_regular(),
            bank: self.bank,
            players: self.players,
            builds: self.builds,
        }
    }
}

trait IntoPlayerData {
    fn into_player_data(
        self,
        dev_cards: crate::gameplay::primitives::dev_card::DevCardData,
    ) -> crate::gameplay::primitives::player::PlayerData;
}

impl IntoPlayerData for crate::gameplay::primitives::resource::ResourceCollection {
    fn into_player_data(
        self,
        dev_cards: crate::gameplay::primitives::dev_card::DevCardData,
    ) -> crate::gameplay::primitives::player::PlayerData {
        crate::gameplay::primitives::player::PlayerData {
            resources: self,
            dev_cards,
        }
    }
}
