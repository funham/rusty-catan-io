use serde::{Deserialize, Serialize};

use crate::gameplay::primitives::{
    build::Build,
    dev_card::DevCardUsage,
    trade::{BankTrade, PersonalTradeOffer, PublicTradeOffer},
};
#[derive(Debug, Serialize, Deserialize)]
pub enum TradeAction {
    Accepted,
    Declined,
}
#[derive(Debug, Serialize, Deserialize)]
pub enum InitialAction {
    ThrowDice,
    UseDevCard(DevCardUsage),
}
#[derive(Debug, Serialize, Deserialize)]
pub enum PostDevCardAction {
    ThrowDice,
}
#[derive(Debug, Serialize, Deserialize)]
pub enum PostDiceThrowAnswer {
    UseDevCard(DevCardUsage),
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Build),
    EndMove,
}
#[derive(Debug, Serialize, Deserialize)]
pub enum FinalStateAnswer {
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Build),
    EndMove,
}
