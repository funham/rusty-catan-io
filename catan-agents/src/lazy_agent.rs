use catan_core::{
    agent::{
        Agent, AgentRequest, AgentResponse,
        action::{
            FinalStateAnswer, InitialAction, PostDevCardAction, PostDiceThrowAnswer, TradeAction,
        },
    },
    gameplay::{game::state::Perspective, primitives::resource::ResourceCollection},
};

#[derive(Debug, Default)]
pub struct LazyAgent;

impl LazyAgent {
    fn choose_initial(perspective: &Perspective) -> AgentResponse {
        let (establishment, road) = perspective
            .builds
            .query()
            .possible_initial_placements(&perspective.field, perspective.player_id)
            .first()
            .expect("there must be a place")
            .clone();

        AgentResponse::Initialization {
            establishment,
            road,
        }
    }
}

impl Agent for LazyAgent {
    fn respond(&mut self, request: AgentRequest) -> AgentResponse {
        match request {
            AgentRequest::Init(_) => AgentResponse::Init(InitialAction::ThrowDice),
            AgentRequest::AfterDevCard(_) => {
                AgentResponse::AfterDevCard(PostDevCardAction::ThrowDice)
            }
            AgentRequest::AfterDiceThrow(_) => {
                AgentResponse::AfterDiceThrow(PostDiceThrowAnswer::EndMove)
            }
            AgentRequest::Rest(_) => AgentResponse::Rest(FinalStateAnswer::EndMove),
            AgentRequest::RobHex(perspective) => {
                let hex = perspective
                    .field
                    .arrangement
                    .hex_enum_iter()
                    .map(|(hex, _)| hex)
                    .find(|hex| *hex != perspective.field.robber_pos)
                    .unwrap_or(perspective.field.robber_pos);
                AgentResponse::RobHex(hex)
            }
            AgentRequest::RobPlayer(perspective) => {
                let player_id = perspective
                    .other_players
                    .first()
                    .map(|player| player.player_id)
                    .unwrap_or(perspective.player_id);
                AgentResponse::RobPlayer(player_id)
            }
            AgentRequest::Initialization(perspective) => Self::choose_initial(&perspective),
            AgentRequest::AnswerTrade {
                perspective: _,
                trade: _,
            } => AgentResponse::AnswerTrade(TradeAction::Declined),
            AgentRequest::DropHalf(perspective) => {
                let number_to_drop = perspective.player_view.resources.total() / 2;
                let mut to_drop = ResourceCollection::default();
                for (resource, number) in perspective.player_view.resources.unroll() {
                    let remaining = number_to_drop - to_drop.total();

                    if remaining == 0 {
                        break;
                    }

                    to_drop[resource] = remaining.min(number);
                }

                AgentResponse::DropHalf(to_drop)
            }
        }
    }
}
