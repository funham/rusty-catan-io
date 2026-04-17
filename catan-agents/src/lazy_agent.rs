use catan_core::{
    agent::{
        action::{
            FinalStateAnswer, InitialAction, PostDevCardAction, PostDiceThrowAnswer, TradeAction,
        },
        Agent, AgentRequest, AgentResponse,
    },
    gameplay::{
        game::state::Perspective,
        primitives::{
            build::{Road, Settlement},
            resource::ResourceCollection,
        },
    },
    topology::Intersection,
};

#[derive(Debug, Default)]
pub struct LazyAgent;

impl LazyAgent {
    fn choose_initial(perspective: &Perspective) -> AgentResponse {
        let occupied = perspective
            .other_players
            .iter()
            .flat_map(|player| {
                player
                    .builds
                    .settlements
                    .iter()
                    .map(|settlement| settlement.pos)
                    .chain(player.builds.cities.iter().map(|city| city.pos))
            })
            .collect::<std::collections::BTreeSet<_>>();

        let placement = perspective
            .field
            .arrangement
            .hex_enum_iter()
            .flat_map(|(hex, _)| hex.vertices().collect::<Vec<_>>())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .find(|candidate| {
                let deadzone = candidate
                    .neighbors()
                    .into_iter()
                    .chain([*candidate])
                    .collect::<std::collections::BTreeSet<Intersection>>();
                occupied.is_disjoint(&deadzone)
            })
            .expect("field should have at least one intersection");

        let road = placement
            .paths()
            .into_iter()
            .next()
            .expect("intersection should have at least one path");

        AgentResponse::Initialization {
            settlement: Settlement { pos: placement },
            road: Road { pos: road },
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
