use std::{cmp::Ordering, collections::BTreeSet};

use catan_core::{
    gameplay::{
        constants,
        field::state::BoardLayout,
        game::{
            command::{
                ChooseRobbedPlayerCommand, DropHalfCommand, InitCommand, InitialPlacementCommand,
                MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand,
            },
            decision::DecisionKind,
            input::{DecisionRequest, PlayerCommand, TradeCommand, TradeResponseCommand},
            trade::TradeSessionId,
            view::{CountingMode, PlayerDecisionContext, PublicPlayerResources, PublicVpKnowledge},
        },
        primitives::{
            Tile,
            build::{Build, EstablishmentType},
            player::{PlayerId, player_ids},
            resource::{Resource, ResourceSet},
            trade::{BankTrade, PlayerTrade},
        },
    },
    topology::{Hex, Intersection},
};

use crate::{bot::BotPolicy, legal, trade};

#[derive(Debug, Default)]
pub struct GreedyAgent {
    id: PlayerId,
    first_initial_resources: Option<BTreeSet<Resource>>,
    attempted_trades: Vec<(ResourceSet, ResourceSet)>,
}

impl GreedyAgent {
    pub fn new(id: impl Into<PlayerId>) -> Self {
        let id = id.into();
        Self {
            id,
            first_initial_resources: None,
            attempted_trades: Vec::new(),
        }
    }
}

impl GreedyAgent {
    fn initial_placement_decision(
        &mut self,
        context: PlayerDecisionContext<'_>,
    ) -> InitialPlacementCommand {
        let action =
            greedy_init_stage_action(&context, self.id, self.first_initial_resources.as_ref());
        if self.first_initial_resources.is_none() {
            self.first_initial_resources = Some(intersection_resources(
                action.settlement_pos(),
                context.public.board,
            ));
        }
        action
    }

    fn init_decision(&mut self, context: PlayerDecisionContext<'_>) -> InitCommand {
        greedy_init_action(context, self.id)
    }

    fn post_dice_decision(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceCommand {
        greedy_after_dice_action(context, self.id)
    }

    fn post_dev_card_decision(
        &mut self,
        _context: PlayerDecisionContext<'_>,
    ) -> PostDevCardCommand {
        PostDevCardCommand::RollDice
    }

    fn regular_decision(&mut self, context: PlayerDecisionContext<'_>) -> RegularCommand {
        greedy_regular_action_with_trades(&context, self.id, &mut self.attempted_trades)
    }

    fn move_robber_decision(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobberCommand {
        greedy_move_robber(context)
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        robber_pos: Hex,
    ) -> ChooseRobbedPlayerCommand {
        greedy_choose_player_to_rob(context, robber_pos)
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfCommand {
        greedy_drop_half(context)
    }

    fn trade_response(
        &mut self,
        context: PlayerDecisionContext<'_>,
        session_id: TradeSessionId,
    ) -> TradeResponseCommand {
        greedy_trade_response(context, session_id)
    }

    fn trade_owner_action(
        &mut self,
        context: PlayerDecisionContext<'_>,
        session_id: TradeSessionId,
    ) -> TradeCommand {
        greedy_trade_owner_action(context, session_id)
    }
}

impl BotPolicy for GreedyAgent {
    fn player_id(&self) -> PlayerId {
        self.id
    }

    fn command_for(
        &mut self,
        request: &DecisionRequest,
        context: PlayerDecisionContext<'_>,
    ) -> Option<PlayerCommand> {
        match request.kind() {
            DecisionKind::InitialPlacement => Some(PlayerCommand::InitialPlacement(
                self.initial_placement_decision(context),
            )),
            DecisionKind::InitCommand => {
                Some(PlayerCommand::InitCommand(self.init_decision(context)))
            }
            DecisionKind::PostDiceCommand => {
                Some(PlayerCommand::PostDice(self.post_dice_decision(context)))
            }
            DecisionKind::PostDevCardCommand => Some(PlayerCommand::PostDevCard(
                self.post_dev_card_decision(context),
            )),
            DecisionKind::RegularCommand => {
                Some(PlayerCommand::Regular(self.regular_decision(context)))
            }
            DecisionKind::MoveRobber => Some(PlayerCommand::MoveRobber(
                self.move_robber_decision(context),
            )),
            DecisionKind::ChooseRobbedPlayer { robber_pos } => Some(
                PlayerCommand::ChooseRobbedPlayer(self.choose_player_to_rob(context, robber_pos)),
            ),
            DecisionKind::DropHalf { .. } => Some(PlayerCommand::DropHalf(self.drop_half(context))),
            DecisionKind::TradeResponse { session } => Some(PlayerCommand::Trade(
                TradeCommand::Respond(self.trade_response(context, session)),
            )),
            DecisionKind::TradeOwnerAction { session } => Some(PlayerCommand::Trade(
                self.trade_owner_action(context, session),
            )),
        }
    }
}

pub fn greedy_drop_half(context: PlayerDecisionContext<'_>) -> DropHalfCommand {
    let number_to_drop = context.private.resources.total() / 2;
    let mut remaining = *context.private.resources;
    let mut to_drop = ResourceSet::default();

    for _ in 0..number_to_drop {
        let Some(resource) = best_resource_to_drop(&context, context.actor, &remaining) else {
            break;
        };

        remaining[resource] -= 1;
        to_drop[resource] += 1;
    }

    DropHalfCommand(to_drop)
}

pub fn greedy_choose_player_to_rob(
    context: PlayerDecisionContext<'_>,
    robber_pos: Hex,
) -> ChooseRobbedPlayerCommand {
    let counting = context.counting();
    let target = legal::legal_rob_targets(&context, robber_pos)
        .into_iter()
        .fold(None, |best, player_id| {
            let score = rob_target_score(&context, player_id, counting);
            match best {
                Some((best_id, best_score)) if best_score >= score => Some((best_id, best_score)),
                _ => Some((player_id, score)),
            }
        })
        .map(|(player_id, _)| player_id)
        .expect("engine must forbid this case");

    ChooseRobbedPlayerCommand(target)
}

pub fn greedy_move_robber(context: PlayerDecisionContext<'_>) -> MoveRobberCommand {
    let counting = context.counting();
    let hex = context
        .public
        .board
        .arrangement
        .hex_iter()
        .filter(|&hex| hex != context.public.board_state.robber_pos)
        .fold(None, |best, hex| {
            let score = move_robber_score(&context, hex, counting);
            match best {
                Some((best_hex, best_score)) if best_score >= score => Some((best_hex, best_score)),
                _ => Some((hex, score)),
            }
        })
        .map(|(hex, _)| hex)
        .expect("there must be a hex without the robber on it");

    MoveRobberCommand(hex)
}

pub fn most_occupied_producing_tile(context: PlayerDecisionContext<'_>) -> Hex {
    use catan_core::gameplay::primitives::Tile;

    context
        .public
        .board
        .arrangement
        .hex_iter()
        .filter(|&h| h != context.public.board_state.robber_pos)
        .fold(None, |best, hex| {
            let score = (
                context
                    .public
                    .players_on_hex(hex)
                    .filter(|&id| id != context.actor)
                    .count(),
                match context.public.board.arrangement[hex] {
                    Tile::Resource { number, .. } => number.as_roll().prob_pts(),
                    Tile::River { number } => number.as_roll().prob_pts() + 3, /* some random *magic* */
                    Tile::Desert => 0,
                },
            );

            match best {
                Some((best_hex, best_score)) if best_score >= score => Some((best_hex, best_score)),
                _ => Some((hex, score)),
            }
        })
        .map(|(hex, _)| hex)
        .expect("some hex must be occupied by at least one other player")
}

pub fn greedy_after_dice_action(
    context: PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> PostDiceCommand {
    if let Some(usage) = legal::first_legal_dev_card_usage(&context) {
        PostDiceCommand::UseDevCard(usage)
    } else {
        PostDiceCommand::RegularCommand(greedy_regular_action(&context, player_id))
    }
}

pub fn greedy_regular_action(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> RegularCommand {
    let mut attempted = Vec::new();
    greedy_regular_action_with_trades(context, player_id, &mut attempted)
}

fn greedy_regular_action_with_trades(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    attempted_trades: &mut Vec<(ResourceSet, ResourceSet)>,
) -> RegularCommand {
    if let Some(build) = best_city_build(context, player_id) {
        return RegularCommand::Build(build);
    }
    if let Some(build) = best_settlement_build(context, player_id) {
        return RegularCommand::Build(build);
    }
    if let Some(build) = best_road_build(context, player_id) {
        return RegularCommand::Build(build);
    }
    if legal::can_buy_dev_card(context) {
        return RegularCommand::BuyDevCard;
    }
    if let Some(trade) = best_player_trade(context, player_id, attempted_trades) {
        attempted_trades.push((trade.give, trade.take));
        return RegularCommand::OfferTrade(trade);
    }
    if let Some(trade) = best_bank_trade(context, player_id) {
        return RegularCommand::TradeWithBank(trade);
    }
    RegularCommand::EndMove
}

fn best_player_trade(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    attempted_trades: &[(ResourceSet, ResourceSet)],
) -> Option<PlayerTrade> {
    let current_score =
        next_objective_score_for_resources(context, player_id, context.private.resources);
    trade::one_card_trade_candidates(context)
        .into_iter()
        .filter_map(|trade| {
            if attempted_trades.contains(&(trade.give, trade.take)) {
                return None;
            }
            if !player_ids(context.public.players.len())
                .filter(|peer| *peer != context.actor)
                .any(|peer| {
                    trade::exact_resources(context, peer)
                        .is_some_and(|resources| resources.has_enough(&trade.take))
                })
            {
                return None;
            }
            let after = trade::resources_after_as_proposer(context.private.resources, &trade)?;
            let score = next_objective_score_for_resources(context, player_id, &after);
            (score > current_score).then_some((trade, score))
        })
        .max_by_key(|(_, score)| *score)
        .map(|(trade, _)| trade)
}

pub fn greedy_trade_response(
    context: PlayerDecisionContext<'_>,
    session_id: TradeSessionId,
) -> TradeResponseCommand {
    let Some(session) = trade::session(&context, session_id) else {
        return TradeResponseCommand::Reject;
    };
    let current_score =
        next_objective_score_for_resources(&context, context.actor, context.private.resources);
    let best_offer = session
        .offers
        .iter()
        .filter(|offer| offer.proposer == session.proposer)
        .filter_map(|offer| {
            if !trade::offer_is_funded(&context, session, offer.id, context.actor) {
                return None;
            }
            let after = trade::resources_after_as_peer(context.private.resources, &offer.trade)?;
            let score = next_objective_score_for_resources(&context, context.actor, &after);
            (score > current_score).then_some((offer.id, score))
        })
        .max_by_key(|(_, score)| *score)
        .map(|(offer_id, _)| offer_id);
    if let Some(offer_id) = best_offer {
        return TradeResponseCommand::Accept { offer_id };
    }

    TradeResponseCommand::Reject
}

pub fn greedy_trade_owner_action(
    context: PlayerDecisionContext<'_>,
    session_id: TradeSessionId,
) -> TradeCommand {
    let Some(session) = trade::session(&context, session_id) else {
        return TradeCommand::Cancel;
    };
    trade::committable_offers(&context, session)
        .into_iter()
        .filter_map(|(offer_id, _)| {
            let offer = session.offer(offer_id)?;
            let after = if offer.proposer == session.proposer {
                trade::resources_after_as_proposer(context.private.resources, &offer.trade)?
            } else {
                trade::resources_after_as_peer(context.private.resources, &offer.trade)?
            };
            let score = next_objective_score_for_resources(&context, context.actor, &after);
            Some((offer_id, score))
        })
        .max_by_key(|(_, score)| *score)
        .map(|(offer_id, _)| TradeCommand::Commit { offer_id })
        .unwrap_or(TradeCommand::Cancel)
}

pub fn greedy_init_action(context: PlayerDecisionContext<'_>, _player_id: PlayerId) -> InitCommand {
    if let Some(usage) = legal::first_legal_dev_card_usage(&context) {
        InitCommand::UseDevCard(usage)
    } else {
        InitCommand::RollDice
    }
}

pub fn greedy_init_stage_action(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    already_acquired: Option<&BTreeSet<Resource>>,
) -> InitialPlacementCommand {
    context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, player_id)
        .into_iter()
        .max_by_key(|action| {
            initial_settlement_score(
                context.public.board,
                action.settlement_pos(),
                already_acquired,
            )
        })
        .expect("there must be an initial placement")
}

fn best_city_build(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Option<Build> {
    legal::legal_city_spots(context, player_id)
        .into_iter()
        .next()
}

fn best_settlement_build(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Option<Build> {
    legal::legal_settlement_spots(context, player_id)
        .into_iter()
        .max_by_key(|build| match build {
            Build::Establishment(establishment) => {
                settlement_production_score(context.public.board, establishment.vtx)
            }
            Build::Road(_) => 0,
        })
}

fn best_road_build(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Option<Build> {
    let roads = legal::legal_road_spots(context, player_id);
    let resources_after_road = context
        .private
        .resources
        .checked_sub(&constants::costs::ROAD)?;

    roads.into_iter().max_by_key(|build| {
        let Build::Road(road) = build else {
            return 0;
        };
        legal::legal_settlement_spots_count_with_extra_road(
            context,
            player_id,
            road.path,
            &resources_after_road,
        )
    })
}

fn best_bank_trade(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Option<BankTrade> {
    let trades = legal::legal_bank_trades(context);
    let Some(search) = &context.search else {
        return trades.into_iter().next();
    };
    let state = search.state();

    trades
        .into_iter()
        .max_by_key(|trade| bank_trade_objective_score(context, player_id, *trade, state))
}

fn bank_trade_objective_score(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    trade: BankTrade,
    state: &catan_core::gameplay::game::state::TableState,
) -> (u8, usize) {
    if !state.bank.can_pay(&trade.from_bank()) {
        return (0, 0);
    }

    let Some(resources_after_trade) =
        legal::resources_after_bank_trade(context.private.resources, trade)
    else {
        return (0, 0);
    };

    next_objective_score_for_resources(context, player_id, &resources_after_trade)
}

fn next_objective_score_for_resources(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &ResourceSet,
) -> (u8, usize) {
    let city_count = legal::legal_city_spots_count_with_resources(context, player_id, resources);
    if city_count > 0 {
        return (4, city_count);
    }

    let settlement_count =
        legal::legal_settlement_spots_count_with_resources(context, player_id, resources);
    if settlement_count > 0 {
        return (3, settlement_count);
    }

    let road_count = legal::legal_road_spots_count_with_resources(context, player_id, resources);
    if road_count > 0 {
        return (2, road_count);
    }
    if resources.has_enough(&constants::costs::DEV_CARD) {
        return (1, 1);
    }
    (0, 0)
}

fn best_resource_to_drop(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &ResourceSet,
) -> Option<Resource> {
    Resource::iter()
        .filter(|resource| resources[*resource] > 0)
        .fold(None, |best, resource| {
            let mut after_drop = *resources;
            after_drop[resource] -= 1;
            let candidate = DropCandidateScore {
                resource,
                hand_score: hand_score(context, player_id, &after_drop),
                demand_score: resource_demand_score_with_hand(
                    context, player_id, resources, resource,
                ),
                current_count: resources[resource],
            };

            match best {
                Some(best)
                    if best_drop_candidate_ordering(candidate, best) != Ordering::Greater =>
                {
                    Some(best)
                }
                _ => Some(candidate),
            }
        })
        .map(|score| score.resource)
}

#[derive(Debug, Clone, Copy)]
struct DropCandidateScore {
    resource: Resource,
    hand_score: (u8, usize),
    demand_score: u16,
    current_count: u16,
}

fn best_drop_candidate_ordering(
    candidate: DropCandidateScore,
    best: DropCandidateScore,
) -> Ordering {
    candidate
        .hand_score
        .cmp(&best.hand_score)
        .then_with(|| best.demand_score.cmp(&candidate.demand_score))
        .then_with(|| candidate.current_count.cmp(&best.current_count))
        .then_with(|| best.resource.cmp(&candidate.resource))
}

fn hand_score(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &ResourceSet,
) -> (u8, usize) {
    next_objective_score_for_resources(context, player_id, resources)
}

fn resource_demand_score(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resource: Resource,
) -> u16 {
    resource_demand_score_with_hand(context, player_id, context.private.resources, resource)
}

fn resource_demand_score_with_hand(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &ResourceSet,
    resource: Resource,
) -> u16 {
    let mut with_resource = *resources;
    with_resource[resource] += 1;
    hand_score_value(hand_score(context, player_id, &with_resource))
        .saturating_sub(hand_score_value(hand_score(context, player_id, resources)))
}

fn hand_score_value((rank, count): (u8, usize)) -> u16 {
    u16::from(rank) * 100 + count.min(99) as u16
}

fn rob_target_score(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    counting: CountingMode,
) -> (u32, u16, u16) {
    let visible_vp = visible_vp_score(context, player_id);
    let resource_total = visible_resource_total(context, player_id);
    let resource_score = match counting {
        CountingMode::Human => 0,
        CountingMode::Counting => exact_public_resources(context, player_id)
            .map(|resources| resource_set_rob_score(context, resources))
            .unwrap_or_default(),
    };

    (resource_score, visible_vp, resource_total)
}

fn resource_set_rob_score(context: &PlayerDecisionContext<'_>, resources: ResourceSet) -> u32 {
    Resource::iter()
        .map(|resource| {
            u32::from(resource_demand_score(context, context.actor, resource))
                * u32::from(resources[resource])
        })
        .sum()
}

fn visible_vp_score(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> u16 {
    let build_vp = context
        .public
        .builds
        .by_player(player_id)
        .establishments
        .iter()
        .map(|establishment| match establishment.stage {
            EstablishmentType::Settlement => constants::vp::SETTLEMENT_VP,
            EstablishmentType::City => constants::vp::CITY_VP,
        })
        .sum::<u16>();
    let award_vp = u16::from(context.public.longest_road_owner == Some(player_id))
        * constants::vp::LONGEST_ROAD_VP
        + u16::from(context.public.largest_army_owner == Some(player_id))
            * constants::vp::LARGEST_ARMY_VP;
    let visible_dev_card_vp = context
        .public
        .players
        .iter()
        .find(|player| player.player_id == player_id)
        .map(|player| match player.dev_cards.victory_points {
            PublicVpKnowledge::Exact(vp) => vp,
            PublicVpKnowledge::Hidden => 0,
        })
        .unwrap_or_default();

    build_vp + award_vp + visible_dev_card_vp
}

fn visible_resource_total(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> u16 {
    if player_id == context.actor {
        return context.private.resources.total();
    }

    context
        .public
        .players
        .iter()
        .find(|player| player.player_id == player_id)
        .map(|player| match player.resources {
            PublicPlayerResources::Exact(resources) => resources.total(),
            PublicPlayerResources::Total(total) => total,
        })
        .unwrap_or_default()
}

fn exact_public_resources(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Option<ResourceSet> {
    if player_id == context.actor {
        return Some(*context.private.resources);
    }

    context
        .public
        .players
        .iter()
        .find(|player| player.player_id == player_id)
        .and_then(|player| match player.resources {
            PublicPlayerResources::Exact(resources) => Some(resources),
            PublicPlayerResources::Total(_) => None,
        })
}

fn move_robber_score(
    context: &PlayerDecisionContext<'_>,
    hex: Hex,
    counting: CountingMode,
) -> (u32, usize, u16) {
    let (opponent_count, production_score) = robber_hex_score(context, hex);
    let target_resource_score = match counting {
        CountingMode::Human => 0,
        CountingMode::Counting => legal::legal_rob_targets(context, hex)
            .into_iter()
            .filter_map(|player_id| exact_public_resources(context, player_id))
            .map(|resources| resource_set_rob_score(context, resources))
            .max()
            .unwrap_or_default(),
    };

    (target_resource_score, opponent_count, production_score)
}

fn robber_hex_score(context: &PlayerDecisionContext<'_>, hex: Hex) -> (usize, u16) {
    let tile_score = match context.public.board.arrangement[hex] {
        Tile::Resource { number, .. } => u16::from(number.as_roll().prob_pts()),
        Tile::River { number } => u16::from(number.as_roll().prob_pts()) + 3,
        Tile::Desert => 0,
    };
    let mut opponent_count = 0;
    let mut production_score = 0;

    for (player_id, builds) in context.public.builds.players_indexed() {
        if player_id == context.actor {
            continue;
        }

        let mut blocks_player = false;
        for establishment in builds
            .establishments
            .iter()
            .filter(|establishment| establishment.vtx.as_arr().contains(&hex))
        {
            blocks_player = true;
            production_score += tile_score * u16::from(establishment.stage.harvest_amount());
        }
        opponent_count += usize::from(blocks_player);
    }

    (opponent_count, production_score)
}

fn initial_settlement_score(
    board: &catan_core::gameplay::field::state::BoardLayout,
    pos: Intersection,
    already_acquired: Option<&BTreeSet<Resource>>,
) -> (usize, u16, usize, u16) {
    let resources = intersection_resource_scores(board, pos);
    let new_resources = resources
        .iter()
        .filter(|(resource, _)| {
            already_acquired
                .map(|acquired| !acquired.contains(resource))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();

    (
        new_resources.len(),
        new_resources.iter().map(|(_, pts)| *pts).sum::<u16>(),
        resources.len(),
        resources.iter().map(|(_, pts)| *pts).sum::<u16>(),
    )
}

fn settlement_production_score(board: &BoardLayout, pos: Intersection) -> u16 {
    intersection_resource_scores(board, pos)
        .into_iter()
        .map(|(_, pts)| pts)
        .sum()
}

fn intersection_resources(intersection: Intersection, board: &BoardLayout) -> BTreeSet<Resource> {
    intersection_resource_scores(board, intersection)
        .into_iter()
        .map(|(resource, _)| resource)
        .collect()
}

fn intersection_resource_scores(
    board: &BoardLayout,
    intersection: Intersection,
) -> Vec<(Resource, u16)> {
    intersection
        .as_arr()
        .into_iter()
        .filter(|hex| hex.norm() <= board.arrangement.radius() as usize)
        .filter_map(|hex| match board.arrangement[hex] {
            Tile::Resource { resource, number } => {
                Some((resource, number.as_roll().prob_pts() as u16))
            }
            Tile::River { .. } | Tile::Desert => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use catan_core::gameplay::{
        game::{
            index::GameIndex,
            state::{GameState, SetupGameState},
            view::{SearchFactory, VisibilityConfig},
        },
        primitives::build::{Establishment, EstablishmentType, Road},
    };

    const P0: PlayerId = PlayerId::new(0);
    const P1: PlayerId = PlayerId::new(1);
    const P2: PlayerId = PlayerId::new(2);
    const P3: PlayerId = PlayerId::new(3);

    #[test]
    fn drop_half_preserves_buildable_city_and_discards_surplus() {
        let mut init = SetupGameState::default();
        place_initial_anywhere(&mut init, P0);
        let mut state = init.finish();
        state
            .transfer_from_bank(
                ResourceSet {
                    sheep: 4,
                    wheat: 2,
                    ore: 3,
                    ..ResourceSet::EMPTY
                },
                P0,
            )
            .expect("bank should fund test resources");

        with_context(&state, P0, CountingMode::Human, |context| {
            let command = greedy_drop_half(context);

            assert_eq!(
                command.0,
                ResourceSet {
                    sheep: 4,
                    ..ResourceSet::EMPTY
                }
            );
        });
    }

    #[test]
    fn human_rob_target_prefers_higher_visible_vp() {
        let mut init = SetupGameState::default();
        let robber_hex = place_players_on_same_hex(&mut init, &[P1, P2]);
        let p2_settlement = init.builds[P2]
            .establishments
            .iter()
            .copied()
            .next()
            .unwrap();
        init.builds
            .try_build(
                P2,
                Build::Establishment(Establishment {
                    vtx: p2_settlement.vtx,
                    stage: EstablishmentType::City,
                }),
            )
            .expect("test city upgrade should be valid");
        let mut state = init.finish();
        state
            .transfer_from_bank(Resource::Brick.into(), P1)
            .unwrap();
        state
            .transfer_from_bank(Resource::Brick.into(), P2)
            .unwrap();

        with_context(&state, P0, CountingMode::Human, |context| {
            let command = greedy_choose_player_to_rob(context, robber_hex);

            assert_eq!(command.0, P2);
        });
    }

    #[test]
    fn counting_rob_target_prefers_holder_of_needed_resources() {
        let mut init = SetupGameState::default();
        place_initial_anywhere(&mut init, P0);
        let robber_hex = place_players_on_same_hex(&mut init, &[P1, P2]);
        let mut state = init.finish();
        state
            .transfer_from_bank(
                ResourceSet {
                    wheat: 2,
                    ore: 2,
                    ..ResourceSet::EMPTY
                },
                P0,
            )
            .unwrap();
        state.transfer_from_bank(Resource::Ore.into(), P1).unwrap();
        state
            .transfer_from_bank(Resource::Brick.into(), P2)
            .unwrap();

        with_context(&state, P0, CountingMode::Counting, |context| {
            let command = greedy_choose_player_to_rob(context, robber_hex);

            assert_eq!(command.0, P1);
        });
    }

    #[test]
    fn human_robber_movement_prefers_blocking_more_opponents() {
        let mut init = SetupGameState::default();
        let crowded_hex = place_players_on_same_hex(&mut init, &[P1, P2]);
        let sparse_hex = place_player_on_different_hex(&mut init, P3, crowded_hex);
        let mut state = init.finish();
        state
            .transfer_from_bank(Resource::Brick.into(), P1)
            .unwrap();
        state
            .transfer_from_bank(Resource::Brick.into(), P2)
            .unwrap();
        state
            .transfer_from_bank(Resource::Brick.into(), P3)
            .unwrap();

        with_context(&state, P0, CountingMode::Human, |context| {
            let command = greedy_move_robber(context.clone());
            let selected_opponents = context
                .public
                .players_on_hex(command.0)
                .filter(|player_id| *player_id != P0)
                .count();
            let sparse_opponents = context
                .public
                .players_on_hex(sparse_hex)
                .filter(|player_id| *player_id != P0)
                .count();

            assert_ne!(command.0, context.public.board_state.robber_pos);
            assert!(selected_opponents > sparse_opponents);
            assert_ne!(command.0, sparse_hex);
            assert!(context.public.players_on_hex(crowded_hex).count() >= selected_opponents);
        });
    }

    #[test]
    fn counting_robber_movement_prefers_needed_resource_target() {
        let mut init = SetupGameState::default();
        place_initial_anywhere(&mut init, P0);
        let ore_hex = place_players_on_same_hex(&mut init, &[P1]);
        let brick_hex = place_player_on_different_hex(&mut init, P2, ore_hex);
        let mut state = init.finish();
        state
            .transfer_from_bank(
                ResourceSet {
                    wheat: 2,
                    ore: 2,
                    ..ResourceSet::EMPTY
                },
                P0,
            )
            .unwrap();
        state.transfer_from_bank(Resource::Ore.into(), P1).unwrap();
        state
            .transfer_from_bank(Resource::Brick.into(), P2)
            .unwrap();

        with_context(&state, P0, CountingMode::Counting, |context| {
            let command = greedy_move_robber(context.clone());
            let selected_targets = legal::legal_rob_targets(&context, command.0);
            let brick_targets = legal::legal_rob_targets(&context, brick_hex);

            assert_ne!(command.0, context.public.board_state.robber_pos);
            assert!(selected_targets.contains(&P1));
            assert!(!brick_targets.contains(&P1));
            assert_ne!(command.0, brick_hex);
            assert!(legal::legal_rob_targets(&context, ore_hex).contains(&P1));
        });
    }

    fn with_context<T>(
        state: &GameState,
        player_id: PlayerId,
        counting: CountingMode,
        f: impl FnOnce(PlayerDecisionContext<'_>) -> T,
    ) -> T {
        let index = GameIndex::rebuild(state);
        let visibility = VisibilityConfig {
            player_mode: counting,
            spectator_mode: counting,
        };
        let factory = catan_core::gameplay::game::view::ContextFactory {
            state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let search = Some(SearchFactory::new(
            state,
            visibility.player_policy(player_id),
            player_id,
        ));

        f(factory.player_decision_context(player_id, search))
    }

    fn place_initial_anywhere(init: &mut SetupGameState, player_id: PlayerId) -> Intersection {
        let action = init
            .builds
            .query()
            .possible_initial_placements(&init.board, player_id)
            .into_iter()
            .next()
            .expect("default board should have an initial placement");
        let (establishment, road) = action.as_builds();
        init.builds
            .try_init_place(player_id, road, establishment)
            .expect("generated placement should be valid");
        establishment.vtx
    }

    fn place_players_on_same_hex(init: &mut SetupGameState, players: &[PlayerId]) -> Hex {
        let candidates = init
            .board
            .arrangement
            .hex_iter()
            .filter(|hex| *hex != init.board_state.robber_pos)
            .collect::<Vec<_>>();

        for hex in candidates {
            let mut candidate = init.clone();
            if players.iter().all(|player_id| {
                try_place_initial_on_hex(&mut candidate, *player_id, hex).is_some()
            }) {
                *init = candidate;
                return hex;
            }
        }

        panic!("test should find a hex that can host requested players");
    }

    fn place_player_on_different_hex(
        init: &mut SetupGameState,
        player_id: PlayerId,
        forbidden: Hex,
    ) -> Hex {
        let candidates = init
            .board
            .arrangement
            .hex_iter()
            .filter(|hex| {
                *hex != forbidden
                    && *hex != init.board_state.robber_pos
                    && matches!(init.board.arrangement[*hex], Tile::Resource { .. })
                    && !setup_hex_has_any_player(init, *hex)
            })
            .collect::<Vec<_>>();

        for hex in candidates {
            let mut candidate = init.clone();
            if try_place_initial_on_hex(&mut candidate, player_id, hex).is_some() {
                *init = candidate;
                return hex;
            }
        }

        panic!("test should find a different legal player hex");
    }

    fn try_place_initial_on_hex(
        init: &mut SetupGameState,
        player_id: PlayerId,
        hex: Hex,
    ) -> Option<Intersection> {
        for vtx in hex
            .vertices_arr()
            .into_iter()
            .filter(|vtx| init.board.intersections().contains(vtx))
        {
            for path in vtx
                .paths_iter()
                .filter(|path| init.board.paths().contains(path))
            {
                let establishment = Establishment {
                    vtx,
                    stage: EstablishmentType::Settlement,
                };
                let road = Road { path };
                let mut candidate = init.builds.clone();
                if candidate
                    .try_init_place(player_id, road, establishment)
                    .is_ok()
                {
                    init.builds = candidate;
                    return Some(vtx);
                }
            }
        }

        None
    }

    fn setup_hex_has_any_player(init: &SetupGameState, hex: Hex) -> bool {
        init.builds.players_indexed().any(|(_, builds)| {
            builds
                .establishments
                .iter()
                .any(|establishment| establishment.vtx.as_arr().contains(&hex))
        })
    }
}
