use serde::{Deserialize, Serialize};

use crate::gameplay::{
    field::state::{BuildCollection, FieldState},
    primitives::{
        bank::BankViewOwned,
        dev_card::DevCardCollection,
        player::{PlayerId, SecuredPlayerData},
        resource::ResourceCollection,
        turn::GameTurn,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PublicPlayerData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameView {
    // board: Board,   // state of the board
    turn: GameTurn, // whose turn it is
    // bank: BankView, // approximate state of the bank (4 possible fullness variants for each card deck: High/Medium/Low/Empty)
    players: Vec<PublicPlayerData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivatePlayerData {
    resources: ResourceCollection,
    dev_cards: DevCardCollection, // active/queued/played
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
