use crate::gameplay::primitives::{
    build::Builds,
    dev_card::DevCardUsage,
    trade::{BankTrade, PersonalTradeOffer, PublicTradeOffer},
};

pub enum TradeAction {
    Accepted,
    Declined,
}
pub enum InitialAction {
    ThrowDice,
    UseDevCard(DevCardUsage),
}
pub enum PostDevCardAction {
    ThrowDice,
}

pub enum PostDiceThrowAnswer {
    UseDevCard(DevCardUsage),
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Builds),
    EndMove,
}

pub enum FinalStateAnswer {
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Builds),
    EndMove,
}
