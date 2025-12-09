use std::collections::BTreeMap;

use crate::{
    gameplay::{field::Field, resource::Resource},
    math::dice::DiceRoller,
};

pub enum DevCardStatus {
    NotReady,
    Ready,
    Played,
}

pub enum DevCardKind {
    Knight,
    YearOfPlenty,
    RoadBuild,
    Monopoly,
    VictoryPoint,
}

pub struct DevCard {
    kind: DevCardKind,
    status: DevCardStatus,
}

pub struct Game {
    field: Field,
    dice: Box<dyn DiceRoller>,
}

impl Game {
    fn transfer_random_card(&mut self) {}
}

pub struct PlayerData {
    resources: BTreeMap<Resource, u8>,
    dev_cards: Vec<DevCard>,
}

pub trait GameActor {}
