use crate::{
    gameplay::{player::PlayerId, resource::Resource},
    topology::{Edge, Hex},
};

pub enum ItemToBuy {
    Road,
    Settlement,
    City,
    DevCard,
}

pub enum TradeKind {
    FOUR,
    THREE,
    TWO,
}

pub struct Trade {
    give: Resource,
    get: Resource,
    kind: TradeKind,
}

pub enum UsableDevCard {
    Knight(Hex),
    YearOfPlenty(Resource, Resource),
    RoadBuild(Edge, Edge),
    Monopoly(Resource),
}

pub struct MoveRascals {
    hex: Hex,
    victim: Option<PlayerId>,
}

pub enum Move {
    Buy(ItemToBuy),
    Trade(Trade),
    Use(UsableDevCard),
}
