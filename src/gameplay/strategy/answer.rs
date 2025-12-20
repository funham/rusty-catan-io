use crate::gameplay::primitives::{
    Robbery,
    build::Buildable,
    dev_card::DevCardUsage,
    trade::{BankTrade, PersonalTradeOffer, PublicTradeOffer},
};

#[derive(Debug, Clone, Copy)]
pub struct RobberyAnswer {
    pub robbery: Robbery,
}

pub enum TradeAnswer {
    Accepted,
    Declined,
}
pub enum InitialAnswer {
    ThrowDice,
    UseKnight(RobberyAnswer),
}
pub enum AfterKnightAnswer {
    ThrowDice,
}

pub enum AfterDiceThrowAnswer {
    UseDevCard(DevCardUsage),
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Buildable),
    EndMove,
}

pub enum FinalStateAnswer {
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Buildable),
    EndMove,
}
