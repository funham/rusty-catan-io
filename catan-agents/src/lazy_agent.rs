use catan_core::{
    agent::{
        Agent,
        action::{InitAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer},
    },
    gameplay::{
        game::{event::PlayerObserver, state::Perspective},
        primitives::{player::PlayerId, resource::ResourceCollection},
    },
};

#[derive(Debug, Default)]
pub struct LazyAgent {
    id: PlayerId,
}

impl LazyAgent {
    pub fn new(id: PlayerId) -> Self {
        Self { id }
    }

    // fn choose_initial(perspective: &Perspective) -> AgentAction {
    //     let (establishment, road) = perspective
    //         .builds
    //         .query()
    //         .possible_initial_placements(&perspective.field, perspective.player_id)
    //         .first()
    //         .expect("there must be a place")
    //         .clone();

    //     AgentAction::Initialization {
    //         establishment,
    //         road,
    //     }
    // }
}

impl PlayerObserver for LazyAgent {
    fn player_id(&self) -> PlayerId {
        self.id
    }
}

// impl Agent for LazyAgent {
//     fn respond(&mut self, request: AgentRequest) -> AgentAction {
//         match request {
//             AgentRequest::Init(_) => AgentAction::Init(InitAction::RollDice),
//             AgentRequest::AfterDevCard(_) => AgentAction::AfterDevCard(PostDevCardAction::RollDice),
//             AgentRequest::AfterDiceThrow(_) => {
//                 AgentAction::AfterDice(PostDiceAction::RegularAction(RegularAction::EndMove))
//             }
//             AgentRequest::Rest(_) => AgentAction::Rest(RegularAction::EndMove),
//             AgentRequest::RobHex(perspective) => {
//                 let hex = perspective
//                     .field
//                     .arrangement
//                     .hex_enum_iter()
//                     .map(|(hex, _)| hex)
//                     .find(|hex| *hex != perspective.field.robber_pos)
//                     .unwrap_or(perspective.field.robber_pos);
//                 AgentAction::RobHex(hex)
//             }
//             AgentRequest::RobPlayer(perspective) => {
//                 let player_id = perspective
//                     .other_players
//                     .first()
//                     .map(|player| player.player_id)
//                     .unwrap_or(perspective.player_id);
//                 AgentAction::RobPlayer(player_id)
//             }
//             AgentRequest::Initialization(perspective) => Self::choose_initial(&perspective),
//             AgentRequest::AnswerTrade {
//                 perspective: _,
//                 trade: _,
//             } => AgentAction::AnswerTrade(TradeAnswer::Declined),
//             AgentRequest::DropHalf(perspective) => {
//                 let number_to_drop = perspective.player_view.resources.total() / 2;
//                 let mut to_drop = ResourceCollection::default();
//                 for (resource, number) in perspective.player_view.resources.unroll() {
//                     let remaining = number_to_drop - to_drop.total();

//                     if remaining == 0 {
//                         break;
//                     }

//                     to_drop[resource] = remaining.min(number);
//                 }

//                 AgentAction::DropHalf(to_drop)
//             }
//         }
//     }
// }
