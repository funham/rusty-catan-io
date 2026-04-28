use serde::{Deserialize, Serialize};

use crate::{
    algorithm,
    gameplay::{
        field::state::{BuildCollection, FieldState},
        game::state::GameState,
        primitives::{
            bank::BankViewOwned,
            build::BoardBuildData,
            dev_card::{DevCardData, SecuredDevCardData},
            player::{PlayerId, SecuredPlayerData},
            resource::ResourceCollection,
            turn::GameTurn,
        },
    },
    topology::Hex,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicPlayerData {
    pub dev_cards: SecuredDevCardData,
    pub resources: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameView {
    // board: Board,   // state of the board
    pub field: FieldState,
    pub turn: GameTurn, // whose turn it is
    pub builds: BoardBuildData,
    // bank: BankView, // approximate state of the bank (4 possible fullness variants for each card deck: High/Medium/Low/Empty)
    pub players: Vec<PublicPlayerData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivatePlayerData {
    pub resources: ResourceCollection,
    pub dev_cards: DevCardData, // active/queued/played
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicPlayerState {
    pub player_id: PlayerId,
    pub public_data: SecuredPlayerData,
    pub builds: BuildCollection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSnapshot {
    pub current_player_id: PlayerId,
    pub rounds_played: u16,
    pub field: FieldState,
    pub bank: BankViewOwned,
    pub players: Vec<PublicPlayerState>,
    pub longest_road_owner: Option<PlayerId>,
    pub largest_army_owner: Option<PlayerId>,
}

impl GameView {
    pub fn is_player_on_hex(&self, id: PlayerId, hex: Hex) -> bool {
        algorithm::is_player_on_hex(hex, self.builds.by_player(id))
    }

    pub fn players_on_hex(&self, hex: Hex) -> Vec<PlayerId> {
        algorithm::players_on_hex(hex, self.builds.players().iter())
            .into_iter()
            .collect()
    }
}
