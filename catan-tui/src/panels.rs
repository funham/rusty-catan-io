//! Text panel builders for the terminal UI.
//!
//! Builds ratatui line buffers for public game state, personal resources/dev cards,
//! discard selection, bank-trade menus, resource pickers, player menus, and game-end summaries.

use crate::{field::FieldRenderer, ratatui_adapter::color as ratatui_color};
use catan_core::gameplay::game::projection::{
    GameProjection, PublicBankDevCardsProjection, PublicBankResourcesProjection,
    PublicPlayerResourcesProjection, TradeSessionProjection,
};
use catan_core::gameplay::primitives::{
    bank::DeckFullnessLevel,
    dev_card::{DevCardData, DevCardKind, UsableDevCard},
    player::PlayerId,
    resource::{Resource, ResourceSet},
    trade::{BankTrade, BankTradeKind},
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::tui::{
    CardGlyph, FinalGameSummaryView, InlineBadge, MiniCardGlyph, append_gap, join_lines_horizontal,
};

#[cfg(test)]
pub fn public_model_lines(model: &GameProjection) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(section_header("bank"));
    lines.extend(bank_panel_lines(model, 80));
    lines.push(Line::from(""));
    lines.push(section_header("players"));
    for player in &model.public.players {
        lines.push(public_player_line(model, player));
    }
    lines.push(Line::from(""));
    lines.push(section_header("awards"));
    lines.push(Line::from(format!(
        "  longest road {:<6} largest army {}",
        player_option_label(model.public.longest_road_owner),
        player_option_label(model.public.largest_army_owner),
    )));
    lines
}

pub fn bank_panel_lines(model: &GameProjection, width: usize) -> Vec<Line<'static>> {
    let mut rows = [Vec::new(), Vec::new(), Vec::new()];

    match &model.public.bank.resources {
        PublicBankResourcesProjection::Exact(resources) => {
            for (idx, resource) in Resource::iter().enumerate() {
                if idx > 0 {
                    append_gap(&mut rows, " ");
                }
                CardGlyph::new(
                    format!("{:>2}", resources[resource].min(99)),
                    bank_resource_deck_style(resource, resources[resource]),
                )
                .push_to_rows(&mut rows);
            }
        }
        PublicBankResourcesProjection::Approx(resources) => {
            for (idx, resource) in Resource::iter().enumerate() {
                if idx > 0 {
                    append_gap(&mut rows, " ");
                }
                let style = if resources[resource] == DeckFullnessLevel::Empty {
                    resource_secondary_style(resource)
                } else {
                    resource_style(resource)
                };
                CardGlyph::new(fullness_symbol(resources[resource]), style).push_to_rows(&mut rows);
            }
        }
    }

    append_gap(&mut rows, " ");
    let dev_deck_style = bank_dev_deck_style(&model.public.bank.dev_cards);
    CardGlyph::new(
        match model.public.bank.dev_cards {
            PublicBankDevCardsProjection::Exact(count) => format!("{:>2}", count.min(99)),
            PublicBankDevCardsProjection::Approx(level) => fullness_symbol(level).to_owned(),
        },
        dev_deck_style,
    )
    .push_to_rows(&mut rows);

    let card_lines = rows.into_iter().map(Line::from).collect::<Vec<_>>();
    add_bank_legend_if_fits(card_lines, width)
}

fn add_bank_legend_if_fits(card_lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    let card_width = card_lines.iter().map(Line::width).max().unwrap_or(0);
    let legend = [
        Line::from("?? 14+"),
        Line::from("? 8-13"),
        Line::from("?! 1-7"),
    ];
    let legend_width = legend.iter().map(Line::width).max().unwrap_or(0);
    if card_width + 2 + legend_width > width {
        return card_lines;
    }

    card_lines
        .into_iter()
        .zip(legend)
        .map(|(mut left, right)| {
            let padding = card_width.saturating_sub(left.width()) + 2;
            left.spans.push(Span::raw(" ".repeat(padding)));
            left.spans.extend(right.spans);
            left
        })
        .collect()
}

pub fn game_summary_lines(summary: &FinalGameSummaryView) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "Game ended",
            Style::default().fg(Color::Green),
        )),
        Line::from(format!("result: {}", summary.result)),
        Line::from(format!("turns: {}", summary.turns_started)),
        Line::from(""),
        Line::from("dice"),
    ];

    for (roll, count) in &summary.dice_counts {
        lines.push(Line::from(format!("{roll:>2}: {count}")));
    }

    lines.extend([
        Line::from(""),
        Line::from(format!(
            "resources distributed: {}",
            summary.resources_distributed
        )),
        Line::from(format!(
            "resources discarded: {}",
            summary.resources_discarded
        )),
        Line::from(format!("resources stolen: {}", summary.resources_stolen)),
        Line::from(format!(
            "builds: roads {} settlements {} cities {}",
            summary.roads_built, summary.settlements_built, summary.cities_built
        )),
        Line::from(format!(
            "dev cards used: knights {} yp {} rb {} monopoly {}",
            summary.knights_used,
            summary.year_of_plenty_used,
            summary.road_build_used,
            summary.monopoly_used
        )),
        Line::from(Span::styled(
            "[press esc to quit]",
            Style::default().fg(Color::Yellow),
        )),
    ]);
    lines
}

pub fn personal_model_lines(model: &GameProjection) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(private) = &model.private {
        lines.push(Line::from(vec![
            Span::raw(format!("you: p{}  ", private.player_id)),
            private_vp_span(model, private.player_id, private.dev_cards.victory_pts),
        ]));
        lines.extend(join_lines_horizontal(
            &resource_card_lines(&private.resources, None),
            Line::from(Span::styled(" │ ", subtle_box_style())),
            &dev_card_lines(&private.dev_cards),
        ));
    } else {
        lines.push(Line::from("no private player data"));
    }
    lines
}

pub fn trade_panel_lines(model: &GameProjection, width: usize) -> Vec<Line<'static>> {
    let Some(session) = model.public.trade_sessions.last() else {
        return vec![Line::from(Span::styled(
            "no active player trade",
            subtle_box_style(),
        ))];
    };

    let mut lines = Vec::new();
    lines.push(trade_session_summary_line(session));
    lines.extend(trade_tree_lines(session, None));
    let hint = if Some(session.proposer) == model.actor {
        "owner: choose confirm, reject counter, or cancel"
    } else {
        "peer: choose accept, reject, or counter"
    };
    lines.push(Line::from(Span::styled(
        truncate_display(hint, width),
        Style::default().fg(Color::Gray),
    )));
    lines
}

pub fn trade_tree_lines(
    session: &TradeSessionProjection,
    selected_offer_index: Option<usize>,
) -> Vec<Line<'static>> {
    trade_tree_lines_for_viewer(session, selected_offer_index, None, None)
}

pub fn trade_tree_lines_for_viewer(
    session: &TradeSessionProjection,
    selected_offer_index: Option<usize>,
    viewer_id: Option<PlayerId>,
    viewer_resources: Option<&ResourceSet>,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (idx, offer) in session.offers.iter().enumerate() {
        let is_counter = offer.proposer != session.proposer;
        let give_resources = resource_set_affordability_for_viewer(
            &offer.trade.give,
            viewer_id == Some(offer.proposer),
            viewer_resources,
        );
        let take_resources = resource_set_affordability_for_viewer(
            &offer.trade.take,
            viewer_id.is_some_and(|viewer_id| viewer_id != offer.proposer),
            viewer_resources,
        );
        let mut spans = Vec::new();
        if selected_offer_index == Some(idx) {
            spans.push(Span::styled("> ", Style::default().fg(Color::Yellow)));
        } else {
            spans.push(Span::raw("  "));
        }
        if is_counter {
            spans.push(Span::styled(">>> ", subtle_box_style()));
        }
        spans.push(Span::styled(
            format!("p{}", offer.proposer),
            player_style(offer.proposer),
        ));
        spans.push(Span::raw(" offers: "));
        push_resource_set_mini_cards(&mut spans, &give_resources);
        spans.push(Span::raw("; for: "));
        push_resource_set_mini_cards(&mut spans, &take_resources);
        spans.push(Span::raw("."));
        if !is_counter {
            spans.push(Span::raw(" "));
            push_trade_response_slots(&mut spans, session, offer.id);
        }
        lines.push(Line::from(spans));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("no offers", subtle_box_style())));
    }
    lines
}

#[derive(Debug, Clone, Copy)]
struct MiniResourceDisplay {
    resource: Resource,
    count: u16,
    affordable: bool,
}

fn resource_set_affordability_for_viewer(
    resources: &ResourceSet,
    paid_by_viewer: bool,
    viewer_resources: Option<&ResourceSet>,
) -> Vec<MiniResourceDisplay> {
    Resource::iter()
        .filter_map(|resource| {
            let count = resources[resource];
            (count > 0).then(|| {
                let affordable =
                    !paid_by_viewer || viewer_resources.is_none_or(|held| held[resource] >= count);
                MiniResourceDisplay {
                    resource,
                    count,
                    affordable,
                }
            })
        })
        .collect()
}

fn push_trade_response_slots(
    spans: &mut Vec<Span<'static>>,
    session: &TradeSessionProjection,
    offer_id: catan_core::gameplay::game::trade::TradeOfferId,
) {
    let mut first = true;
    for (player_index, response) in session.responses.iter().enumerate() {
        let Some(response) = response else {
            continue;
        };
        if !first {
            spans.push(Span::raw(" "));
        }
        first = false;
        let player_id = PlayerId::try_from(player_index).expect("player index should fit in u8");
        let symbol = match response {
            catan_core::gameplay::game::trade::TradeResponseState::Waiting => "-",
            catan_core::gameplay::game::trade::TradeResponseState::Rejected => "x",
            catan_core::gameplay::game::trade::TradeResponseState::Accepted {
                offer_id: accepted,
            } if *accepted == offer_id => "v",
            catan_core::gameplay::game::trade::TradeResponseState::Countered { .. } => "!",
            catan_core::gameplay::game::trade::TradeResponseState::Accepted { .. } => "-",
        };
        spans.push(Span::styled(format!("({symbol})"), player_style(player_id)));
    }
}

fn push_resource_set_mini_cards(spans: &mut Vec<Span<'static>>, resources: &[MiniResourceDisplay]) {
    if resources.is_empty() {
        spans.push(Span::styled("-", subtle_box_style()));
        return;
    }
    for display in resources {
        let style = if display.affordable {
            resource_style(display.resource)
        } else {
            resource_secondary_style(display.resource)
        };
        MiniCardGlyph::new(display.count.min(99).to_string())
            .face_style(style)
            .bracket_style(style)
            .push_to(spans);
    }
}

pub fn player_trade_builder_lines(
    _available: &ResourceSet,
    give: &ResourceSet,
    take: &ResourceSet,
    selected_resource: usize,
    editing_give: bool,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("player trade"),
        Line::from(
            "left/right resource; tab switches side; up/down count; enter offers; esc cancels",
        ),
        Line::from(if editing_give {
            "editing: give"
        } else {
            "editing: take"
        }),
    ];
    lines.push(Line::from(if editing_give { "> give" } else { "  give" }));
    lines.extend(resource_card_lines(give, None));
    if editing_give {
        lines.push(resource_selector_line(selected_resource));
    }
    lines.push(Line::from(if editing_give { "  take" } else { "> take" }));
    lines.extend(resource_card_lines(take, None));
    if !editing_give {
        lines.push(resource_selector_line(selected_resource));
    }
    lines
}

fn trade_session_summary_line(session: &TradeSessionProjection) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("s{} ", session.id.0),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(format!("p{} public  ", session.proposer)),
        Span::raw(format!("offers {}", session.offers.len())),
    ])
}

pub fn snapshot_state_lines(
    model: &GameProjection,
    width: u16,
    active_player: Option<PlayerId>,
) -> Vec<Line<'static>> {
    let Some(state) = &model.snapshot_state else {
        return vec![Line::from("no exact snapshot state available")];
    };
    let width = snapshot_box_width(width);

    let mut lines = Vec::new();
    lines.extend(snapshot_turn_box_lines(model, width));
    lines.extend(snapshot_bank_box_lines(model, width));

    for player_id in catan_core::gameplay::primitives::player::player_ids(state.players.count()) {
        lines.extend(snapshot_player_box_lines(
            model,
            player_id,
            width,
            active_player,
        ));
    }

    lines
}

fn snapshot_turn_box_lines(model: &GameProjection, width: usize) -> Vec<Line<'static>> {
    let state = model
        .snapshot_state
        .as_ref()
        .expect("snapshot_turn_box_lines requires exact snapshot state");
    vec![
        box_top("turn", width),
        box_text_line(
            format!(
                "turns {:>3}  rounds {:>2}  LR {}  LA {}",
                0,
                0,
                player_option_label(state.builds.longest_road()),
                player_option_label(state.players.best_army())
            ),
            width,
        ),
        box_bottom(width),
    ]
}

fn snapshot_bank_box_lines(model: &GameProjection, width: usize) -> Vec<Line<'static>> {
    let state = model
        .snapshot_state
        .as_ref()
        .expect("snapshot_bank_box_lines requires exact snapshot state");
    let mut lines = vec![bank_box_top(
        state.bank.resources.total(),
        state.bank.dev_cards.len(),
        width,
    )];
    lines.extend(wrap_box_lines(snapshot_bank_content_lines(state), width));
    lines.push(box_bottom(width));
    lines
}

fn snapshot_bank_content_lines(
    state: &catan_core::gameplay::game::state::TableState,
) -> Vec<Line<'static>> {
    let resources = resource_card_lines(&state.bank.resources, None);
    let dev_cards = dev_deck_card_lines(&state.bank.dev_cards);
    let left_width = resources.iter().map(Line::width).max().unwrap_or(0);
    let left = [
        resources.first().cloned().unwrap_or_else(|| Line::from("")),
        resources.get(1).cloned().unwrap_or_else(|| Line::from("")),
        resources.get(2).cloned().unwrap_or_else(|| Line::from("")),
        Line::from(format!(
            "next {}",
            dev_deck_next_summary(&state.bank.dev_cards)
        )),
    ];

    left.into_iter()
        .zip(dev_cards)
        .map(|(left, right)| bank_split_line(left, right, left_width))
        .collect()
}

fn bank_split_line(left: Line<'static>, right: Line<'static>, left_width: usize) -> Line<'static> {
    let mut spans = fit_line_to_width(left, left_width).spans;
    spans.push(Span::raw(" "));
    spans.push(Span::styled("|", subtle_box_style()));
    spans.push(Span::raw(" "));
    spans.extend(right.spans);
    Line::from(spans)
}

fn snapshot_player_box_lines(
    model: &GameProjection,
    player_id: PlayerId,
    width: usize,
    active_player: Option<PlayerId>,
) -> Vec<Line<'static>> {
    let state = model
        .snapshot_state
        .as_ref()
        .expect("snapshot_player_box_lines requires exact snapshot state");
    let player = state.players.get(player_id);
    let is_active = active_player == Some(player_id);
    let mut title = format!("p{player_id}");
    if state.builds.longest_road() == Some(player_id) {
        title.push_str(" LR");
    }
    if state.players.best_army() == Some(player_id) {
        title.push_str(" LA");
    }

    let border_style = player_box_border_style(is_active);
    let mut lines = vec![box_top_styled(&title, width, border_style)];
    lines.extend(wrap_box_lines_styled(
        resource_card_lines(player.resources(), None),
        width,
        border_style,
    ));
    lines.extend(wrap_box_lines_styled(
        dev_card_compact_lines(player.dev_cards()),
        width,
        border_style,
    ));
    lines.push(box_bottom_styled(width, border_style));
    lines
}

pub fn public_player_lines(
    model: &GameProjection,
    player: &catan_core::gameplay::game::projection::PublicPlayerProjection,
) -> Vec<Line<'static>> {
    let style = player_style(player.player_id);
    let not_played_dev_cards = player.queued_dev_cards + player.active_dev_cards;
    let army_style = achievement_style(model.public.largest_army_owner == Some(player.player_id));
    let road_style = achievement_style(model.public.longest_road_owner == Some(player.player_id));

    let mut summary = vec![
        Span::styled(format!("p{}", player.player_id), style),
        Span::raw(format!(
            "  VP:{}  ",
            visible_victory_points(model, player.player_id)
        )),
    ];
    push_unknown_resource_card_inline(&mut summary);
    summary.push(Span::raw(" "));
    summary.push(public_resource_hand_count_span(player));
    summary.push(Span::raw("  "));
    push_dev_card_inline(&mut summary);
    summary.push(Span::raw(" "));
    summary.push(Span::styled(
        format!("{:02}", not_played_dev_cards.min(99)),
        dev_card_style(),
    ));

    vec![
        Line::from(summary),
        Line::from(vec![
            Span::styled("KN ", army_style),
            Span::styled(
                player.played_dev_cards.knight.min(99).to_string(),
                army_style,
            ),
            Span::raw("  "),
            Span::styled("LR ", road_style),
            Span::styled(player.longest_road_length.min(99).to_string(), road_style),
        ]),
    ]
}

#[cfg(test)]
fn public_player_line(
    model: &GameProjection,
    player: &catan_core::gameplay::game::projection::PublicPlayerProjection,
) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    spans.extend(public_player_lines(model, player).remove(0).spans);
    Line::from(spans)
}

fn public_resource_hand_count_span(
    player: &catan_core::gameplay::game::projection::PublicPlayerProjection,
) -> Span<'static> {
    match &player.resources {
        PublicPlayerResourcesProjection::Exact(resources) => {
            Span::raw(format!("{:02}", resources.total().min(99)))
        }
        PublicPlayerResourcesProjection::Total(total) => {
            Span::raw(format!("{:02}", (*total).min(99)))
        }
    }
}

fn private_vp_span(
    model: &GameProjection,
    player_id: PlayerId,
    secret_vp_cards: u16,
) -> Span<'static> {
    let public_vp = visible_victory_points(model, player_id);
    if secret_vp_cards > 0 {
        Span::raw(format!("VP: {public_vp} ({})", public_vp + secret_vp_cards))
    } else {
        Span::raw(format!("VP: {public_vp}"))
    }
}

fn visible_victory_points(model: &GameProjection, player_id: PlayerId) -> u16 {
    let build_vp = model
        .public
        .builds
        .iter()
        .find(|builds| builds.player_id == player_id)
        .map(|builds| {
            builds
                .establishments
                .iter()
                .map(|establishment| match establishment.stage {
                    catan_core::gameplay::primitives::build::EstablishmentType::Settlement => 1,
                    catan_core::gameplay::primitives::build::EstablishmentType::City => 2,
                })
                .sum::<u16>()
        })
        .unwrap_or(0);
    let longest_road = (model.public.longest_road_owner == Some(player_id)) as u16 * 2;
    let largest_army = (model.public.largest_army_owner == Some(player_id)) as u16 * 2;
    build_vp + longest_road + largest_army
}

#[cfg(test)]
fn section_header(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(label.to_ascii_uppercase(), subtle_box_style()))
}

fn player_option_label(player_id: Option<PlayerId>) -> String {
    player_id
        .map(|player_id| format!("p{player_id}"))
        .unwrap_or_else(|| "-".to_owned())
}

fn snapshot_box_width(width: u16) -> usize {
    usize::from(width.max(12))
}

fn box_content_width(width: usize) -> usize {
    width.saturating_sub(4)
}

fn box_top(title: &str, width: usize) -> Line<'static> {
    box_top_styled(title, width, subtle_box_style())
}

fn box_top_styled(title: &str, width: usize, style: Style) -> Line<'static> {
    let title = format!(" {} ", title.to_ascii_uppercase());
    let title = truncate_display(title, width.saturating_sub(2));
    let fill = width.saturating_sub(2 + title.chars().count());
    Line::from(Span::styled(
        format!("╭{title}{}╮", "─".repeat(fill)),
        style,
    ))
}

fn bank_box_top(resource_total: u16, dev_total: usize, width: usize) -> Line<'static> {
    let label = format!(" BANK ({resource_total}, {dev_total}) ");
    let label = truncate_display(label, width.saturating_sub(2));
    let fill = width.saturating_sub(2 + label.chars().count());
    let mut spans = vec![Span::styled("╭", subtle_box_style())];
    push_colored_bank_title(&mut spans, &label, resource_total, dev_total);
    spans.push(Span::styled("─".repeat(fill), subtle_box_style()));
    spans.push(Span::styled("╮", subtle_box_style()));
    Line::from(spans)
}

fn push_colored_bank_title(
    spans: &mut Vec<Span<'static>>,
    label: &str,
    resource_total: u16,
    dev_total: usize,
) {
    let resource = resource_total.to_string();
    let dev = dev_total.to_string();
    let Some((before_resource, after_resource)) = label.split_once(&resource) else {
        spans.push(Span::styled(label.to_owned(), subtle_box_style()));
        return;
    };
    let Some((between, after_dev)) = after_resource.split_once(&dev) else {
        spans.push(Span::styled(label.to_owned(), subtle_box_style()));
        return;
    };
    spans.push(Span::styled(before_resource.to_owned(), subtle_box_style()));
    spans.push(Span::styled(resource, Style::default().fg(Color::Green)));
    spans.push(Span::styled(between.to_owned(), subtle_box_style()));
    spans.push(Span::styled(dev, Style::default().fg(Color::Magenta)));
    spans.push(Span::styled(after_dev.to_owned(), subtle_box_style()));
}

fn box_bottom(width: usize) -> Line<'static> {
    box_bottom_styled(width, subtle_box_style())
}

fn box_bottom_styled(width: usize, style: Style) -> Line<'static> {
    Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(width.saturating_sub(2))),
        style,
    ))
}

fn box_text_line(text: impl Into<String>, width: usize) -> Line<'static> {
    let content_width = box_content_width(width);
    let text = truncate_display(text.into(), content_width);
    let padding = content_width.saturating_sub(text.chars().count());
    Line::from(vec![
        box_left(),
        Span::raw(text),
        Span::raw(" ".repeat(padding)),
        box_right(),
    ])
}

fn wrap_box_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    wrap_box_lines_styled(lines, width, subtle_box_style())
}

fn wrap_box_lines_styled(
    lines: Vec<Line<'static>>,
    width: usize,
    border_style: Style,
) -> Vec<Line<'static>> {
    let content_width = box_content_width(width);
    lines
        .into_iter()
        .map(|line| {
            if line.width() > content_width {
                return box_text_line_styled(line.to_string(), width, border_style);
            }
            let padding = content_width.saturating_sub(line.width());
            let mut spans = vec![box_left_styled(border_style)];
            spans.extend(line.spans);
            spans.push(Span::raw(" ".repeat(padding)));
            spans.push(box_right_styled(border_style));
            Line::from(spans)
        })
        .collect()
}

fn box_left() -> Span<'static> {
    box_left_styled(subtle_box_style())
}

fn box_right() -> Span<'static> {
    box_right_styled(subtle_box_style())
}

fn box_left_styled(style: Style) -> Span<'static> {
    Span::styled("│ ", style)
}

fn box_right_styled(style: Style) -> Span<'static> {
    Span::styled(" │", style)
}

fn box_text_line_styled(text: impl Into<String>, width: usize, style: Style) -> Line<'static> {
    let content_width = box_content_width(width);
    let text = truncate_display(text.into(), content_width);
    let padding = content_width.saturating_sub(text.chars().count());
    Line::from(vec![
        box_left_styled(style),
        Span::raw(text),
        Span::raw(" ".repeat(padding)),
        box_right_styled(style),
    ])
}

fn subtle_box_style() -> Style {
    Style::default().fg(Color::Indexed(244))
}

fn active_box_style() -> Style {
    Style::default().fg(Color::Indexed(39))
}

fn achievement_style(has_achievement: bool) -> Style {
    if has_achievement {
        Style::default().fg(Color::Yellow)
    } else {
        subtle_box_style()
    }
}

fn player_box_border_style(is_active: bool) -> Style {
    if is_active {
        active_box_style()
    } else {
        subtle_box_style()
    }
}

fn truncate_display(text: impl Into<String>, max_width: usize) -> String {
    let text = text.into();
    if text.chars().count() <= max_width {
        return text;
    }
    text.chars().take(max_width).collect()
}

fn fit_line_to_width(line: Line<'static>, width: usize) -> Line<'static> {
    if line.width() > width {
        return Line::from(truncate_display(line.to_string(), width));
    }
    let padding = width.saturating_sub(line.width());
    let mut spans = line.spans;
    spans.push(Span::raw(" ".repeat(padding)));
    Line::from(spans)
}

fn dev_deck_next_summary(dev_cards: &[DevCardKind]) -> String {
    let preview = dev_cards
        .iter()
        .take(7)
        .map(dev_card_kind_abbrev)
        .collect::<Vec<_>>();
    if preview.is_empty() {
        "-".to_owned()
    } else {
        preview.join(" ")
    }
}

fn dev_deck_card_lines(dev_cards: &[DevCardKind]) -> Vec<Line<'static>> {
    let counts = dev_deck_counts(dev_cards);
    let mut rows = [Vec::new(), Vec::new(), Vec::new()];
    let mut count = Vec::new();

    for (idx, (label, amount)) in [
        ("KN", counts.knight),
        ("YP", counts.yop),
        ("M", counts.monopoly),
        ("RB", counts.roadbuild),
        ("VP", counts.victory),
    ]
    .into_iter()
    .enumerate()
    {
        if idx > 0 {
            append_gap(&mut rows, " ");
            count.push(Span::raw(" "));
        }
        CardGlyph::new(label, dev_card_style()).push_to_rows(&mut rows);
        count.push(Span::styled(
            format!("{:^4}", amount.min(99)),
            Style::default().fg(Color::Magenta),
        ));
    }

    vec![
        Line::from(rows[0].clone()),
        Line::from(rows[1].clone()),
        Line::from(rows[2].clone()),
        Line::from(count),
    ]
}

#[derive(Default)]
struct DevDeckCounts {
    knight: u16,
    yop: u16,
    monopoly: u16,
    roadbuild: u16,
    victory: u16,
}

fn dev_deck_counts(dev_cards: &[DevCardKind]) -> DevDeckCounts {
    let mut counts = DevDeckCounts::default();
    for card in dev_cards {
        match card {
            DevCardKind::VictoryPoint => counts.victory += 1,
            DevCardKind::Usable(UsableDevCard::Knight) => counts.knight += 1,
            DevCardKind::Usable(UsableDevCard::YearOfPlenty) => counts.yop += 1,
            DevCardKind::Usable(UsableDevCard::Monopoly) => counts.monopoly += 1,
            DevCardKind::Usable(UsableDevCard::RoadBuild) => counts.roadbuild += 1,
        }
    }
    counts
}

pub fn resource_card_lines(
    resources: &ResourceSet,
    selected_discard: Option<&ResourceSet>,
) -> Vec<Line<'static>> {
    let mut rows = [Vec::new(), Vec::new(), Vec::new()];
    let mut selected = Vec::new();

    for (idx, resource) in Resource::iter().enumerate() {
        if idx > 0 {
            append_gap(&mut rows, " ");
            selected.push(Span::raw(" "));
        }
        let style = if resources[resource] == 0 {
            resource_secondary_style(resource)
        } else {
            resource_style(resource)
        };
        CardGlyph::new(format!("{:02}", resources[resource].min(99)), style)
            .push_to_rows(&mut rows);
        if let Some(discard) = selected_discard {
            selected.push(Span::styled(
                format!(" {:02} ", discard[resource].min(99)),
                style,
            ));
        }
    }

    let mut lines = rows.into_iter().map(Line::from).collect::<Vec<_>>();
    if selected_discard.is_some() {
        lines.push(Line::from(selected));
    }
    lines
}

pub fn dev_card_lines(dev_cards: &DevCardData) -> Vec<Line<'static>> {
    dev_card_compact_lines(dev_cards)
}

fn dev_card_compact_lines(dev_cards: &DevCardData) -> Vec<Line<'static>> {
    let mut rows = [Vec::new(), Vec::new(), Vec::new()];

    for (idx, card) in [
        UsableDevCard::Knight,
        UsableDevCard::YearOfPlenty,
        UsableDevCard::Monopoly,
        UsableDevCard::RoadBuild,
    ]
    .into_iter()
    .enumerate()
    {
        if idx > 0 {
            append_gap(&mut rows, " ");
        }
        CardGlyph::new(dev_card_abbrev(card), dev_card_glyph_style(dev_cards, card))
            .index_top(dev_cards.used[card], used_dev_card_count_style())
            .index_mid(dev_cards.active[card], active_dev_card_count_style())
            .index_bottom(dev_cards.queued[card], queued_dev_card_count_style())
            .push_to_rows(&mut rows);
    }

    append_gap(&mut rows, " ");
    CardGlyph::new(
        "VP",
        victory_point_dev_card_glyph_style(dev_cards.victory_pts),
    )
    .index_mid(
        dev_cards.victory_pts,
        victory_point_dev_card_count_style(dev_cards.victory_pts),
    )
    .push_to_rows(&mut rows);

    rows.into_iter().map(Line::from).collect()
}

pub fn discard_personal_lines(
    player_id: impl Into<PlayerId>,
    resources: &ResourceSet,
    _dev_cards: &DevCardData,
    selected: &ResourceSet,
    required: u16,
    selected_resource: usize,
) -> Vec<Line<'static>> {
    let player_id = player_id.into();
    let mut lines = vec![
        Line::from(format!("you: p{player_id}")),
        Line::from(format!("discard {} / {} cards", selected.total(), required)),
    ];
    lines.extend(discard_resource_card_lines(
        resources,
        selected,
        selected_resource,
    ));
    lines
}

fn discard_resource_card_lines(
    resources: &ResourceSet,
    selected: &ResourceSet,
    selected_resource: usize,
) -> Vec<Line<'static>> {
    let mut lines = resource_card_lines(resources, None);
    let mut selector = Vec::new();
    let mut selected_counts = Vec::new();
    for (idx, resource) in Resource::iter().enumerate() {
        if idx > 0 {
            selector.push(Span::raw(" "));
            selected_counts.push(Span::raw(" "));
        }
        let style = resource_style(resource);
        let marker = if idx == selected_resource {
            "^^^^"
        } else {
            "    "
        };
        selector.push(Span::styled(marker, style));
        selected_counts.push(Span::styled(
            format!("{:^4}", selected[resource].min(99)),
            style,
        ));
    }
    lines.push(Line::from(selector));
    lines.push(Line::from(selected_counts));
    lines
}

pub fn bank_trade_menu_lines(options: &[BankTrade], selected: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("bank trade"),
        Line::from("up/down select; enter confirms; esc cancels"),
    ];
    let visible_rows = 11;
    let half_window = visible_rows / 2;
    let start = selected.saturating_sub(half_window);
    let end = (start + visible_rows).min(options.len());

    for (idx, trade) in options.iter().enumerate().skip(start).take(end - start) {
        let marker = if idx == selected { "> " } else { "  " };
        lines.push(bank_trade_menu_line(marker, *trade));
    }
    lines
}

fn bank_trade_menu_line(marker: &str, trade: BankTrade) -> Line<'static> {
    let mut spans = vec![Span::raw(marker.to_owned())];
    let give_style = resource_style(trade.give);
    MiniCardGlyph::new(bank_trade_rate_count(trade.kind).to_string())
        .face_style(give_style)
        .bracket_style(give_style)
        .push_to(&mut spans);
    spans.push(Span::raw(" -> "));
    let take_style = resource_style(trade.take);
    MiniCardGlyph::new("1")
        .face_style(take_style)
        .bracket_style(take_style)
        .push_to(&mut spans);
    Line::from(spans)
}

fn bank_trade_rate_count(kind: BankTradeKind) -> u16 {
    match kind {
        BankTradeKind::BankGeneric => 4,
        BankTradeKind::PortGeneric => 3,
        BankTradeKind::PortSpecific => 2,
    }
}

pub fn resource_choice_lines(
    title: &str,
    selected_resource: usize,
    locked_resource: Option<Resource>,
) -> Vec<Line<'static>> {
    let mut rows = [Vec::new(), Vec::new(), Vec::new()];
    for (idx, resource) in Resource::iter().enumerate() {
        if idx > 0 {
            append_gap(&mut rows, " ");
        }
        let style = resource_choice_style(resource, idx == selected_resource, locked_resource);
        CardGlyph::new(resource_abbrev(resource), style).push_to_rows(&mut rows);
    }

    let mut lines = vec![
        Line::from(title.to_owned()),
        Line::from("left/right select; enter locks; esc cancels or clears first pick"),
    ];
    if let Some(resource) = locked_resource {
        lines.push(Line::from(vec![
            Span::raw("first: "),
            Span::styled(format!("{resource:?}"), resource_secondary_style(resource)),
        ]));
    }
    lines.extend(rows.into_iter().map(Line::from));
    lines.push(resource_selector_line(selected_resource));
    lines
}

fn resource_choice_style(resource: Resource, hovered: bool, locked: Option<Resource>) -> Style {
    if hovered {
        resource_style(resource)
    } else if locked == Some(resource) {
        resource_secondary_style(resource)
    } else {
        subtle_box_style()
    }
}

fn resource_secondary_style(resource: Resource) -> Style {
    resource_style(resource).add_modifier(Modifier::DIM)
}

fn bank_resource_deck_style(resource: Resource, count: u16) -> Style {
    if count == 0 {
        resource_secondary_style(resource)
    } else {
        resource_style(resource)
    }
}

fn bank_dev_deck_style(dev_cards: &PublicBankDevCardsProjection) -> Style {
    match dev_cards {
        PublicBankDevCardsProjection::Exact(0)
        | PublicBankDevCardsProjection::Approx(DeckFullnessLevel::Empty) => {
            dev_card_secondary_style()
        }
        PublicBankDevCardsProjection::Exact(_) | PublicBankDevCardsProjection::Approx(_) => {
            dev_card_style()
        }
    }
}

fn resource_selector_line(selected_resource: usize) -> Line<'static> {
    let mut selector = Vec::new();
    for (idx, resource) in Resource::iter().enumerate() {
        if idx > 0 {
            selector.push(Span::raw(" "));
        }
        let marker = if idx == selected_resource {
            "^^^^"
        } else {
            "    "
        };
        selector.push(Span::styled(marker, resource_style(resource)));
    }
    Line::from(selector)
}

pub fn player_menu_lines(candidates: &[PlayerId], selected: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("rob player"),
        Line::from("up/down select; enter confirms; esc cancels"),
    ];
    for (idx, player_id) in candidates.iter().enumerate() {
        let marker = if idx == selected { "> " } else { "  " };
        lines.push(Line::from(format!("{marker}p{player_id}")));
    }
    lines
}

pub fn adjust_discard_selection(
    available: &ResourceSet,
    selected: &mut ResourceSet,
    resource: Resource,
    delta: i8,
) {
    if delta > 0 {
        selected[resource] = (selected[resource] + delta as u16).min(available[resource]);
    } else {
        selected[resource] = selected[resource].saturating_sub(delta.unsigned_abs() as u16);
    }
}

fn push_unknown_resource_card_inline(spans: &mut Vec<Span<'static>>) {
    InlineBadge::new(
        MiniCardGlyph::new("?")
            .face_style(resource_style(Resource::Wheat))
            .bracket_styles(
                resource_style(Resource::Brick),
                resource_style(Resource::Wood),
            )
            .spans(),
    )
    .push_to(spans);
}

fn push_dev_card_inline(spans: &mut Vec<Span<'static>>) {
    InlineBadge::new(
        MiniCardGlyph::new("?")
            .face_style(dev_card_style())
            .bracket_style(dev_card_style())
            .spans(),
    )
    .push_to(spans);
}

fn dev_card_abbrev(card: UsableDevCard) -> &'static str {
    match card {
        UsableDevCard::Knight => "KN",
        UsableDevCard::YearOfPlenty => "YP",
        UsableDevCard::Monopoly => "M",
        UsableDevCard::RoadBuild => "RB",
    }
}

fn dev_card_kind_abbrev(card: &DevCardKind) -> &'static str {
    match card {
        DevCardKind::VictoryPoint => "VP",
        DevCardKind::Usable(card) => dev_card_abbrev(*card),
    }
}

fn fullness_symbol(level: DeckFullnessLevel) -> &'static str {
    match level {
        DeckFullnessLevel::High => "??",
        DeckFullnessLevel::Medium => " ?",
        DeckFullnessLevel::Low => "?!",
        DeckFullnessLevel::Empty => " !",
    }
}

fn resource_abbrev(resource: Resource) -> &'static str {
    match resource {
        Resource::Brick => "Br",
        Resource::Wood => "Wo",
        Resource::Wheat => "Wh",
        Resource::Sheep => "Sh",
        Resource::Ore => "Or",
    }
}

fn resource_style(resource: Resource) -> Style {
    Style::default().fg(ratatui_color(FieldRenderer::resource_color(resource)))
}

fn player_style(player_id: PlayerId) -> Style {
    Style::default().fg(ratatui_color(FieldRenderer::player_color(player_id)))
}

fn dev_card_style() -> Style {
    Style::default().fg(Color::Magenta)
}

fn dev_card_secondary_style() -> Style {
    dev_card_style().add_modifier(Modifier::DIM)
}

fn empty_dev_card_style() -> Style {
    subtle_box_style()
}

fn dev_card_glyph_style(dev_cards: &DevCardData, card: UsableDevCard) -> Style {
    if dev_cards.active[card] > 0 {
        dev_card_style()
    } else if dev_cards.queued[card] > 0 {
        dev_card_secondary_style()
    } else {
        empty_dev_card_style()
    }
}

fn victory_point_dev_card_glyph_style(count: u16) -> Style {
    if count > 0 {
        dev_card_style()
    } else {
        empty_dev_card_style()
    }
}

fn victory_point_dev_card_count_style(count: u16) -> Style {
    if count > 0 {
        active_dev_card_count_style()
    } else {
        empty_dev_card_style()
    }
}

fn used_dev_card_count_style() -> Style {
    subtle_box_style()
}

fn active_dev_card_count_style() -> Style {
    dev_card_style()
}

fn queued_dev_card_count_style() -> Style {
    dev_card_secondary_style()
}

#[cfg(test)]
mod tests {
    use catan_core::gameplay::game::projection::GameProjection;
    use catan_core::gameplay::primitives::{
        dev_card::DevCardData,
        resource::{Resource, ResourceSet},
        trade::{BankTrade, BankTradeKind, PlayerTrade},
    };
    use catan_core::gameplay::{
        game::{
            event::ObserverNotificationContext,
            index::GameIndex,
            state::SetupGameState,
            view::{ContextFactory, VisibilityConfig},
        },
        primitives::{dev_card::DevCardKind, dev_card::UsableDevCard, player::PlayerId},
    };
    use ratatui::style::{Color, Style};

    use super::{
        adjust_discard_selection, bank_panel_lines, bank_trade_menu_lines, dev_card_lines,
        discard_personal_lines, personal_model_lines, player_style, player_trade_builder_lines,
        public_model_lines, resource_card_lines, resource_secondary_style, snapshot_state_lines,
    };

    #[test]
    fn card_lines_render_resource_and_dev_counts() {
        let resources = ResourceSet {
            brick: 1,
            wood: 2,
            wheat: 13,
            sheep: 0,
            ore: 5,
        };
        let rendered_resources = resource_card_lines(&resources, None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(rendered_resources[0].contains("┌──┐"));
        assert!(rendered_resources[1].contains("│01│"));
        assert!(rendered_resources[1].contains("│13│"));

        let dev_cards = DevCardData::default();
        let rendered_dev = dev_card_lines(&dev_cards)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(rendered_dev[1].contains("│KN│"));
        assert!(rendered_dev[1].contains("│YP│"));
        assert!(rendered_dev[1].contains("│ M│") || rendered_dev[1].contains("│M │"));
        assert!(rendered_dev[1].contains("│VP│"));
    }

    #[test]
    fn dev_card_counts_use_state_specific_styles() {
        let mut dev_cards = DevCardData::default();
        dev_cards.used[UsableDevCard::Knight] = 1;
        dev_cards.active[UsableDevCard::Knight] = 2;
        dev_cards.queued[UsableDevCard::Knight] = 3;

        let lines = dev_card_lines(&dev_cards);
        assert!(lines[0].spans.iter().any(|span| {
            span.content.as_ref().trim() == "1" && span.style == super::subtle_box_style()
        }));
        assert!(lines[1].spans.iter().any(|span| {
            span.content.as_ref().trim() == "2" && span.style.fg == Some(Color::Magenta)
        }));
        assert!(lines[2].spans.iter().any(|span| {
            span.content.as_ref().trim() == "3"
                && span.style.fg == Some(Color::Magenta)
                && span
                    .style
                    .add_modifier
                    .contains(ratatui::style::Modifier::DIM)
        }));
    }

    #[test]
    fn dev_card_glyph_color_ignores_used_and_prefers_active_then_queued() {
        let mut dev_cards = DevCardData::default();
        dev_cards.used[UsableDevCard::Knight] = 4;
        dev_cards.queued[UsableDevCard::YearOfPlenty] = 1;
        dev_cards.active[UsableDevCard::Monopoly] = 1;

        let lines = dev_card_lines(&dev_cards);

        assert!(lines[1].spans.iter().any(|span| {
            span.content.as_ref() == "KN" && span.style == super::subtle_box_style()
        }));
        assert!(lines[1].spans.iter().any(|span| {
            span.content.as_ref() == "YP"
                && span.style.fg == Some(Color::Magenta)
                && span
                    .style
                    .add_modifier
                    .contains(ratatui::style::Modifier::DIM)
        }));
        assert!(lines[1].spans.iter().any(|span| {
            span.content.as_ref().trim() == "M" && span.style.fg == Some(Color::Magenta)
        }));
    }

    #[test]
    fn private_dev_card_glyphs_never_use_default_or_white_style() {
        let mut dev_cards = DevCardData::default();
        dev_cards.used[UsableDevCard::Knight] = 1;
        dev_cards.queued[UsableDevCard::YearOfPlenty] = 1;
        dev_cards.active[UsableDevCard::Monopoly] = 1;

        for line in dev_card_lines(&dev_cards) {
            for span in line.spans {
                let content = span.content.as_ref();
                if content.trim().is_empty() {
                    continue;
                }
                assert_ne!(span.style.fg, None, "unstyled dev-card span: {content:?}");
                assert_ne!(
                    span.style.fg,
                    Some(Color::White),
                    "white dev-card span: {content:?}"
                );
            }
        }
    }

    #[test]
    fn empty_private_dev_card_glyphs_are_grey_including_victory_points() {
        let lines = dev_card_lines(&DevCardData::default());

        for label in ["KN", "YP", "M", "RB", "VP"] {
            assert!(lines[1].spans.iter().any(|span| {
                span.content.as_ref().trim() == label && span.style == super::subtle_box_style()
            }));
        }
    }

    #[test]
    fn mini_card_glyph_allows_independent_face_and_bracket_styles() {
        let glyph = super::MiniCardGlyph::new("4")
            .face_style(Style::default().fg(Color::Blue))
            .bracket_style(Style::default().fg(Color::White));
        let spans = glyph.spans();

        assert_eq!(
            spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            "[4]"
        );
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert_eq!(spans[1].style.fg, Some(Color::Blue));
        assert_eq!(spans[2].style.fg, Some(Color::White));
    }

    #[test]
    fn trade_tree_lines_render_prime_offers_counters_and_player_colored_response_slots() {
        let session = catan_core::gameplay::game::projection::TradeSessionProjection {
            id: catan_core::gameplay::game::trade::TradeSessionId(0),
            proposer: PlayerId::new(0),
            offers: vec![
                catan_core::gameplay::game::projection::TradeOfferProjection {
                    id: catan_core::gameplay::game::trade::TradeOfferId(0),
                    proposer: PlayerId::new(0),
                    trade: PlayerTrade {
                        give: ResourceSet::from(Resource::Ore),
                        take: ResourceSet::from(Resource::Wood),
                    },
                },
                catan_core::gameplay::game::projection::TradeOfferProjection {
                    id: catan_core::gameplay::game::trade::TradeOfferId(1),
                    proposer: PlayerId::new(3),
                    trade: PlayerTrade {
                        give: ResourceSet::from(Resource::Brick),
                        take: ResourceSet::from(Resource::Wheat),
                    },
                },
            ],
            responses: vec![
                None,
                Some(catan_core::gameplay::game::trade::TradeResponseState::Waiting),
                Some(
                    catan_core::gameplay::game::trade::TradeResponseState::Accepted {
                        offer_id: catan_core::gameplay::game::trade::TradeOfferId(0),
                    },
                ),
                Some(
                    catan_core::gameplay::game::trade::TradeResponseState::Countered {
                        offer_id: catan_core::gameplay::game::trade::TradeOfferId(1),
                    },
                ),
            ],
            version: 0,
            open: true,
        };

        let lines = super::trade_tree_lines(&session, Some(0));
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("p0 offers:"));
        assert!(rendered.contains("for:"));
        assert!(rendered.contains("(-)"));
        assert!(rendered.contains("(v)"));
        assert!(rendered.contains("(!)"));
        assert!(rendered.contains(">>> p3 offers:"));
        let first_line = &lines[0];
        assert!(first_line.spans.iter().any(|span| {
            span.content.as_ref() == "(-)" && span.style == player_style(PlayerId::new(1))
        }));
        assert!(first_line.spans.iter().any(|span| {
            span.content.as_ref() == "(v)" && span.style == player_style(PlayerId::new(2))
        }));
        assert!(first_line.spans.iter().any(|span| {
            span.content.as_ref() == "(!)" && span.style == player_style(PlayerId::new(3))
        }));
    }

    #[test]
    fn trade_tree_resource_mini_cards_show_counts_not_resource_ordinals() {
        let session = catan_core::gameplay::game::projection::TradeSessionProjection {
            id: catan_core::gameplay::game::trade::TradeSessionId(0),
            proposer: PlayerId::new(0),
            offers: vec![
                catan_core::gameplay::game::projection::TradeOfferProjection {
                    id: catan_core::gameplay::game::trade::TradeOfferId(0),
                    proposer: PlayerId::new(0),
                    trade: PlayerTrade {
                        give: ResourceSet {
                            brick: 1,
                            sheep: 2,
                            ..ResourceSet::EMPTY
                        },
                        take: ResourceSet {
                            ore: 3,
                            ..ResourceSet::EMPTY
                        },
                    },
                },
            ],
            responses: vec![
                None,
                Some(catan_core::gameplay::game::trade::TradeResponseState::Waiting),
            ],
            version: 0,
            open: true,
        };

        let line = super::trade_tree_lines(&session, None)
            .into_iter()
            .next()
            .expect("trade line should render");
        let rendered = line.to_string();

        assert!(rendered.contains("[1]"));
        assert!(rendered.contains("[2]"));
        assert!(rendered.contains("[3]"));
        assert!(!rendered.contains("[0]"));
        assert!(!rendered.contains("[3][3]"));
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == "2" && span.style == super::resource_style(Resource::Sheep)
        }));
    }

    #[test]
    fn trade_tree_dims_resources_the_viewer_cannot_pay() {
        let session = catan_core::gameplay::game::projection::TradeSessionProjection {
            id: catan_core::gameplay::game::trade::TradeSessionId(0),
            proposer: PlayerId::new(0),
            offers: vec![
                catan_core::gameplay::game::projection::TradeOfferProjection {
                    id: catan_core::gameplay::game::trade::TradeOfferId(0),
                    proposer: PlayerId::new(0),
                    trade: PlayerTrade {
                        give: ResourceSet {
                            brick: 1,
                            ..ResourceSet::EMPTY
                        },
                        take: ResourceSet {
                            ore: 2,
                            ..ResourceSet::EMPTY
                        },
                    },
                },
            ],
            responses: vec![
                None,
                Some(catan_core::gameplay::game::trade::TradeResponseState::Waiting),
            ],
            version: 0,
            open: true,
        };
        let viewer_resources = ResourceSet {
            ore: 1,
            ..ResourceSet::EMPTY
        };

        let line = super::trade_tree_lines_for_viewer(
            &session,
            None,
            Some(PlayerId::new(1)),
            Some(&viewer_resources),
        )
        .into_iter()
        .next()
        .expect("trade line should render");

        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == "1" && span.style == super::resource_style(Resource::Brick)
        }));
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == "2"
                && span.style == super::resource_secondary_style(Resource::Ore)
        }));
    }

    #[test]
    fn player_trade_builder_lines_show_only_give_take_and_selector_on_edited_set() {
        let available = ResourceSet {
            ore: 2,
            wood: 1,
            ..ResourceSet::EMPTY
        };
        let give = ResourceSet::from(Resource::Ore);
        let take = ResourceSet::from(Resource::Wood);
        let rendered = player_trade_builder_lines(&available, &give, &take, 4, true)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("player trade"));
        assert!(rendered.contains("esc cancels"));
        assert!(!rendered.contains("available"));
        assert!(rendered.contains("give"));
        assert!(rendered.contains("take"));
        assert!(rendered.contains("^^"));

        let lines = player_trade_builder_lines(&available, &give, &take, 4, true);
        let rendered_lines = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        let give_index = rendered_lines
            .iter()
            .position(|line| line.contains("> give"))
            .expect("give label should render");
        let take_index = rendered_lines
            .iter()
            .position(|line| line.contains("  take"))
            .expect("take label should render");
        assert!(rendered_lines[give_index + 4].contains("^^^^"));
        assert!(
            rendered_lines
                .iter()
                .skip(take_index + 1)
                .all(|line| !line.contains("^^^^"))
        );
    }

    #[test]
    fn selected_trade_set_zero_cards_use_secondary_outline() {
        let available = ResourceSet {
            ore: 2,
            ..ResourceSet::EMPTY
        };
        let lines = player_trade_builder_lines(
            &available,
            &ResourceSet::EMPTY,
            &ResourceSet::EMPTY,
            4,
            true,
        );
        let give_top = lines
            .iter()
            .skip_while(|line| !line.to_string().contains("> give"))
            .nth(1)
            .expect("give top row should render");

        assert!(give_top.spans.iter().any(|span| {
            span.content.as_ref() == "┌──┐" && span.style == resource_secondary_style(Resource::Ore)
        }));
    }

    #[test]
    fn personal_panel_places_dev_cards_right_of_resources_after_divider() {
        let mut state = SetupGameState::default().finish();
        state
            .transfer_from_bank(
                ResourceSet {
                    brick: 2,
                    wood: 1,
                    ..ResourceSet::EMPTY
                },
                0,
            )
            .unwrap();
        state
            .players
            .get_mut(0)
            .dev_cards_add(DevCardKind::Usable(UsableDevCard::Knight));
        state.players.get_mut(0).dev_cards_reset_queue();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let model = GameProjection::from_observer(
            ObserverNotificationContext::Player {
                public: factory.public_view(visibility.player_policy(PlayerId::new(0))),
                private: factory.private_view(PlayerId::new(0)),
            },
            false,
        );

        let rendered = personal_model_lines(&model)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!rendered.contains("|||"));
        assert!(rendered.contains(" │ "));
        assert!(rendered.contains("│KN│"));
    }

    #[test]
    fn zero_resource_cards_use_secondary_style() {
        let resources = ResourceSet {
            brick: 1,
            ..ResourceSet::EMPTY
        };
        let lines = resource_card_lines(&resources, None);

        assert!(lines[1].spans.iter().any(|span| {
            span.content.as_ref() == "00" && span.style == resource_secondary_style(Resource::Wood)
        }));
    }

    #[test]
    fn personal_panel_uses_single_three_row_separator_without_dev_card_subscript() {
        let mut state = SetupGameState::default().finish();
        state
            .players
            .get_mut(0)
            .dev_cards_add(DevCardKind::Usable(UsableDevCard::Knight));
        state.players.get_mut(0).dev_cards_reset_queue();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let model = GameProjection::from_observer(
            ObserverNotificationContext::Player {
                public: factory.public_view(visibility.player_policy(PlayerId::new(0))),
                private: factory.private_view(PlayerId::new(0)),
            },
            false,
        );

        let rendered = personal_model_lines(&model)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!rendered.contains("|||"));
        assert!(!rendered.contains(" KN     YP     M      RB     VP "));
        assert_eq!(dev_card_lines(&DevCardData::default()).len(), 3);
    }

    #[test]
    fn discard_selection_is_bounded_by_available_resources() {
        let available = ResourceSet {
            brick: 2,
            ..ResourceSet::EMPTY
        };
        let mut selected = ResourceSet::EMPTY;

        adjust_discard_selection(&available, &mut selected, Resource::Brick, 1);
        adjust_discard_selection(&available, &mut selected, Resource::Brick, 1);
        adjust_discard_selection(&available, &mut selected, Resource::Brick, 1);
        assert_eq!(selected.brick, 2);

        adjust_discard_selection(&available, &mut selected, Resource::Brick, -1);
        adjust_discard_selection(&available, &mut selected, Resource::Brick, -1);
        adjust_discard_selection(&available, &mut selected, Resource::Brick, -1);
        assert_eq!(selected.brick, 0);
    }

    #[test]
    fn discard_lines_show_selector_counts_and_total() {
        let resources = ResourceSet {
            brick: 2,
            wood: 1,
            ..ResourceSet::EMPTY
        };
        let selected = ResourceSet {
            brick: 1,
            ..ResourceSet::EMPTY
        };
        let dev_cards = DevCardData::default();
        let lines = discard_personal_lines(0, &resources, &dev_cards, &selected, 2, 0)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(
            lines
                .iter()
                .any(|line| line.contains("discard 1 / 2 cards"))
        );
        assert!(!lines.iter().any(|line| line.contains("KN")));
        assert!(!lines.iter().any(|line| line.contains("VP")));
        assert!(lines.iter().any(|line| line.contains("^^^^")));
        assert!(lines.iter().any(|line| line.contains(" 1  ")));
        assert!(lines.iter().any(|line| line.contains(" 0  ")));
    }

    #[test]
    fn bank_trade_menu_shows_instructions_and_selected_trade() {
        let options = vec![
            BankTrade {
                give: Resource::Brick,
                take: Resource::Wood,
                kind: BankTradeKind::BankGeneric,
            },
            BankTrade {
                give: Resource::Wheat,
                take: Resource::Ore,
                kind: BankTradeKind::PortGeneric,
            },
        ];
        let lines = bank_trade_menu_lines(&options, 1)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("up/down select")));
        assert!(lines.iter().any(|line| line.contains("> [3] -> [1]")));
        assert!(!lines.iter().any(|line| line.contains("Wheat -> Ore")));
    }

    #[test]
    fn snapshot_state_lines_render_dashboard_and_player_boxes() {
        let mut state = SetupGameState::default().finish();
        state.bank.dev_cards = vec![
            DevCardKind::Usable(UsableDevCard::Knight),
            DevCardKind::Usable(UsableDevCard::YearOfPlenty),
            DevCardKind::VictoryPoint,
        ];
        state
            .transfer_from_bank(
                ResourceSet {
                    brick: 2,
                    wood: 1,
                    ..ResourceSet::EMPTY
                },
                0,
            )
            .unwrap();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let model = GameProjection::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            true,
        );

        let lines = snapshot_state_lines(&model, 60, Some(PlayerId::new(0)))
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("TURN")));
        assert!(lines.iter().any(|line| line.contains("BANK (92, 3)")));
        assert!(lines.iter().any(|line| line.contains("P0")));
        assert!(lines.iter().any(|line| line.contains("│02│")));
        assert!(lines.iter().any(|line| line.contains("│KN│")));
        assert!(lines.iter().any(|line| line.contains("|")));
        assert!(lines.iter().any(|line| line.contains("next KN YP VP")));
    }

    #[test]
    fn public_model_lines_focus_on_bank_players_and_journal_ready_state() {
        let mut state = SetupGameState::default().finish();
        state
            .transfer_from_bank(
                ResourceSet {
                    brick: 2,
                    wood: 1,
                    ..ResourceSet::EMPTY
                },
                0,
            )
            .unwrap();
        state.bank.resources.wheat = 10;
        state.bank.dev_cards.truncate(7);
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let model = GameProjection::from_observer(
            ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        );

        let lines = public_model_lines(&model)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        let rendered = lines.join("\n");

        assert!(rendered.contains("BANK"));
        assert!(rendered.contains("PLAYERS"));
        assert!(rendered.contains("VP:"));
        assert!(rendered.contains("│??│"));
        assert!(rendered.contains("│ ?│"));
        assert!(!rendered.contains("visible vp"));
        assert!(!rendered.contains("hand"));
        assert!(!rendered.contains("resources"));
        assert!(!rendered.contains("development deck"));
        assert!(!rendered.contains("board r"));
        assert!(!rendered.contains("robber"));
        assert!(!rendered.contains("ACTIONS"));
        assert!(!rendered.contains("roll | end | buy dev"));
        assert!(!rendered.contains("TRADE / DEV"));
    }

    #[test]
    fn bank_panel_adds_fullness_legend_when_width_allows() {
        let state = SetupGameState::default().finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let model = GameProjection::from_observer(
            ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        );

        let rendered = super::bank_panel_lines(&model, 80)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert_eq!(rendered.len(), 3);
        assert!(rendered.iter().any(|line| line.contains("?? 14+")));
        assert!(rendered.iter().any(|line| line.contains("? 8-13")));
        assert!(rendered.iter().any(|line| line.contains("?! 1-7")));
        assert!(!rendered.iter().any(|line| line.contains("! 0")));
    }

    #[test]
    fn bank_empty_decks_use_secondary_style() {
        let mut state = SetupGameState::default().finish();
        state.bank.resources.brick = 0;
        state.bank.dev_cards.clear();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let model = GameProjection::from_observer(
            ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        );

        let lines = bank_panel_lines(&model, 80);
        assert!(lines[1].spans.iter().any(|span| {
            span.content.as_ref() == " !" && span.style == resource_secondary_style(Resource::Brick)
        }));
        assert!(lines[1].spans.iter().any(|span| {
            span.content.as_ref() == " !" && span.style == super::dev_card_secondary_style()
        }));
    }
}
