//! Text panel builders for the terminal UI.
//!
//! Builds ratatui line buffers for public game state, personal resources/dev cards,
//! discard selection, bank-trade menus, resource pickers, player menus, and game-end stats.

use catan_agents::remote_agent::{UiModel, UiPublicBankResources, UiPublicPlayerResources};
use catan_core::gameplay::{
    game::event::GameEndPlayerStats,
    primitives::{
        bank::DeckFullnessLevel,
        dev_card::{DevCardData, UsableDevCard},
        player::PlayerId,
        resource::{Resource, ResourceCollection},
        trade::{BankTrade, BankTradeKind},
    },
};
use catan_render::{adapters::ratatui::color as ratatui_color, field::FieldRenderer};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

pub(crate) fn public_model_lines(model: &UiModel) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(section_header("turn"));
    lines.push(Line::from(format!(
        "  actor {:<6} robber {:?}",
        player_option_label(model.actor),
        model.public.board_state.robber_pos.index().to_spiral(),
    )));
    lines.push(Line::from(format!(
        "  longest road {:<6} largest army {}",
        player_option_label(model.public.longest_road_owner),
        player_option_label(model.public.largest_army_owner),
    )));
    lines.push(Line::from(format!(
        "  board r{} / {} tiles",
        model.public.board.field_radius,
        model.public.board.tiles.len()
    )));
    lines.push(Line::from(""));

    lines.push(section_header("bank"));
    lines.push(bank_resources_line(model));
    lines.push(Line::from(""));
    lines.push(section_header("players"));
    for player in &model.public.players {
        lines.push(public_player_line(player));
    }
    lines.push(Line::from(""));
    lines.push(section_header("actions"));
    lines.push(Line::from("  roll | end | buy dev"));
    lines.push(Line::from("  build road | settlement | city"));
    lines.push(Line::from(""));
    lines.push(section_header("trade / dev"));
    lines.push(Line::from("  bt menu | bt [give] [take] [G4|G3|S2]"));
    lines.push(Line::from("  kn | yp | m | rb"));
    lines
}

pub(crate) fn game_ended_lines(
    model: &UiModel,
    winner_id: PlayerId,
    turn_no: u64,
    stats: &[GameEndPlayerStats],
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "Game ended",
            Style::default().fg(Color::Green),
        )),
        Line::from(format!("winner: p{winner_id} | turns: {turn_no}")),
        Line::from(format!(
            "robber {} | LR {:?} | LA {:?}",
            model.public.board_state.robber_pos.index().to_spiral(),
            model.public.longest_road_owner,
            model.public.largest_army_owner
        )),
        Line::from(""),
        Line::from("final stats"),
        Line::from("┌───┬──┬──┬──┬──┬──┬──┬──┬──┬─────────┐"),
        Line::from("│P  │VP│B │A │S │C │R │L │K │Tags     │"),
        Line::from("├───┼──┼──┼──┼──┼──┼──┼──┼──┼─────────┤"),
    ];

    let mut sorted = stats.to_vec();
    sorted.sort_by_key(|stats| (std::cmp::Reverse(stats.total_vp), stats.player_id));
    for stats in sorted {
        let mut tags = Vec::new();
        if stats.player_id == winner_id {
            tags.push("WIN");
        }
        if stats.has_longest_road {
            tags.push("LR");
        }
        if stats.has_largest_army {
            tags.push("LA");
        }
        lines.push(Line::from(format!(
            "│p{:<2}│{:>2}│{:>2}│{:>2}│{:>2}│{:>2}│{:>2}│{:>2}│{:>2}│{:<9}│",
            stats.player_id,
            stats.total_vp,
            stats.build_and_dev_card_vp,
            stats.award_vp,
            stats.settlements,
            stats.cities,
            stats.roads,
            stats.longest_road_length,
            stats.knights_used,
            tags.join(" "),
        )));
    }

    lines.extend([
        Line::from("└───┴──┴──┴──┴──┴──┴──┴──┴──┴─────────┘"),
        Line::from(""),
        Line::from("B=base VP  A=award VP"),
        Line::from("S=set C=city R=road"),
        Line::from("L=longest K=knights"),
        Line::from(Span::styled(
            "[press esc to quit]",
            Style::default().fg(Color::Yellow),
        )),
    ]);
    lines
}

pub(crate) fn personal_model_lines(model: &UiModel) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(private) = &model.private {
        lines.push(Line::from(format!("you: p{}", private.player_id)));
        lines.push(Line::from(format!(
            "resources: ({})",
            private.resources.total()
        )));
        lines.extend(resource_card_lines(&private.resources, None));
        lines.push(Line::from(""));
        lines.push(Line::from("development"));
        lines.extend(dev_card_lines(&private.dev_cards));
    } else {
        lines.push(Line::from("no private player data"));
    }
    lines
}

fn bank_resources_line(model: &UiModel) -> Line<'static> {
    let mut spans = vec![Span::raw("bank: resources ")];
    match &model.public.bank.resources {
        UiPublicBankResources::Exact(resources) => {
            push_resource_values(&mut spans, resources, |count| count.to_string());
        }
        UiPublicBankResources::Approx(resources) => {
            for (idx, resource) in Resource::iter().into_iter().enumerate() {
                if idx > 0 {
                    spans.push(Span::raw(" "));
                }
                push_resource_value(&mut spans, resource, fullness_symbol(resources[resource]));
            }
        }
    }
    spans.push(Span::raw(" | dev "));
    let dev = match &model.public.bank.resources {
        UiPublicBankResources::Exact(_) => model.public.bank.dev_card_count.to_string(),
        UiPublicBankResources::Approx(_) => {
            fullness_symbol(deck_fullness(model.public.bank.dev_card_count)).to_owned()
        }
    };
    spans.push(Span::styled(dev, Style::default().fg(Color::Magenta)));
    Line::from(spans)
}

fn public_player_line(player: &catan_agents::remote_agent::UiPublicPlayer) -> Line<'static> {
    let style = player_style(player.player_id);
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(format!("p{}", player.player_id), style),
        Span::raw(format!(
            "  vp {:<2}  hand ",
            victory_points_label(player.victory_points)
        )),
    ];
    match &player.resources {
        UiPublicPlayerResources::Exact(resources) => {
            push_resource_values(&mut spans, resources, |count| count.to_string());
        }
        UiPublicPlayerResources::Total(total) => {
            spans.push(Span::styled(format!("{total:>2}"), style));
        }
    }
    spans.push(Span::raw(format!(
        "  dev {}/{}",
        player.active_dev_cards, player.queued_dev_cards
    )));
    Line::from(spans)
}

fn section_header(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_ascii_uppercase(),
        Style::default().fg(Color::Cyan),
    ))
}

fn player_option_label(player_id: Option<PlayerId>) -> String {
    player_id
        .map(|player_id| format!("p{player_id}"))
        .unwrap_or_else(|| "-".to_owned())
}

fn victory_points_label(victory_points: Option<u16>) -> String {
    victory_points
        .map(|victory_points| victory_points.to_string())
        .unwrap_or_else(|| "?".to_owned())
}

pub(crate) fn resource_card_lines(
    resources: &ResourceCollection,
    selected_drop: Option<&ResourceCollection>,
) -> Vec<Line<'static>> {
    let mut top = Vec::new();
    let mut middle = Vec::new();
    let mut bottom = Vec::new();
    let mut selected = Vec::new();

    for (idx, resource) in Resource::iter().into_iter().enumerate() {
        if idx > 0 {
            for spans in [&mut top, &mut middle, &mut bottom, &mut selected] {
                spans.push(Span::raw(" "));
            }
        }
        let style = resource_style(resource);
        top.push(Span::styled("┌──┐", style));
        middle.push(Span::styled(
            format!("│{:02}│", resources[resource].min(99)),
            style,
        ));
        bottom.push(Span::styled("└──┘", style));
        if let Some(drop) = selected_drop {
            selected.push(Span::styled(
                format!(" {:02} ", drop[resource].min(99)),
                style,
            ));
        }
    }

    let mut lines = vec![Line::from(top), Line::from(middle), Line::from(bottom)];
    if selected_drop.is_some() {
        lines.push(Line::from(selected));
    }
    lines
}

pub(crate) fn dev_card_lines(dev_cards: &DevCardData) -> Vec<Line<'static>> {
    let mut top = Vec::new();
    let mut middle = Vec::new();
    let mut bottom = Vec::new();
    let mut labels = Vec::new();

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
            push_dev_card_gap(&mut top, &mut middle, &mut bottom, &mut labels);
        }
        push_dev_card(
            &mut top,
            &mut middle,
            &mut bottom,
            &mut labels,
            dev_card_abbrev(card),
            [
                Some(dev_cards.used[card]),
                Some(dev_cards.active[card]),
                Some(dev_cards.queued[card]),
            ],
        );
    }

    push_dev_card_gap(&mut top, &mut middle, &mut bottom, &mut labels);
    push_dev_card(
        &mut top,
        &mut middle,
        &mut bottom,
        &mut labels,
        "VP",
        [None, Some(dev_cards.victory_pts), None],
    );

    vec![
        Line::from(top),
        Line::from(middle),
        Line::from(bottom),
        Line::from(labels),
    ]
}

pub(crate) fn drop_personal_lines(
    player_id: PlayerId,
    resources: &ResourceCollection,
    dev_cards: &DevCardData,
    selected: &ResourceCollection,
    required: u16,
    selected_resource: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!("you: p{player_id}")),
        Line::from(format!("drop {} / {} cards", selected.total(), required)),
    ];
    lines.extend(drop_resource_card_lines(
        resources,
        selected,
        selected_resource,
    ));
    lines.push(Line::from(""));
    lines.extend(dev_card_lines(dev_cards));
    lines
}

fn drop_resource_card_lines(
    resources: &ResourceCollection,
    selected: &ResourceCollection,
    selected_resource: usize,
) -> Vec<Line<'static>> {
    let mut lines = resource_card_lines(resources, None);
    let mut selector = Vec::new();
    let mut selected_counts = Vec::new();
    for (idx, resource) in Resource::iter().into_iter().enumerate() {
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

pub(crate) fn bank_trade_menu_lines(options: &[BankTrade], selected: usize) -> Vec<Line<'static>> {
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
    let rate = match trade.kind {
        BankTradeKind::BankGeneric => "4:1",
        BankTradeKind::PortGeneric => "3:1",
        BankTradeKind::PortSpecific => "2:1",
    };
    Line::from(vec![
        Span::raw(marker.to_owned()),
        Span::raw(format!("{rate} ")),
        Span::styled(format!("{:?}", trade.give), resource_style(trade.give)),
        Span::raw(" -> "),
        Span::styled(format!("{:?}", trade.take), resource_style(trade.take)),
    ])
}

pub(crate) fn resource_picker_lines(selected_resource: usize) -> Vec<Line<'static>> {
    let resources = ResourceCollection {
        brick: 1,
        wood: 1,
        wheat: 1,
        sheep: 1,
        ore: 1,
    };
    let mut lines = vec![Line::from("left/right select; enter confirms; esc cancels")];
    lines.extend(resource_card_lines(&resources, None));
    lines.push(resource_selector_line(selected_resource));
    lines
}

fn resource_selector_line(selected_resource: usize) -> Line<'static> {
    let mut selector = Vec::new();
    for (idx, resource) in Resource::iter().into_iter().enumerate() {
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

pub(crate) fn player_menu_lines(candidates: &[PlayerId], selected: usize) -> Vec<Line<'static>> {
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

pub(crate) fn adjust_drop_selection(
    available: &ResourceCollection,
    selected: &mut ResourceCollection,
    resource: Resource,
    delta: i8,
) {
    if delta > 0 {
        selected[resource] = (selected[resource] + delta as u16).min(available[resource]);
    } else {
        selected[resource] = selected[resource].saturating_sub(delta.unsigned_abs() as u16);
    }
}

fn push_dev_card_gap(
    top: &mut Vec<Span<'static>>,
    middle: &mut Vec<Span<'static>>,
    bottom: &mut Vec<Span<'static>>,
    labels: &mut Vec<Span<'static>>,
) {
    for spans in [top, middle, bottom, labels] {
        spans.push(Span::raw(" "));
    }
}

fn push_dev_card(
    top: &mut Vec<Span<'static>>,
    middle: &mut Vec<Span<'static>>,
    bottom: &mut Vec<Span<'static>>,
    labels: &mut Vec<Span<'static>>,
    label: &'static str,
    counts: [Option<u16>; 3],
) {
    let style = Style::default().fg(Color::Magenta);
    top.push(Span::styled("┌──┐", style));
    top.push(Span::raw(format!("{:>2}", count_label(counts[0]))));
    middle.push(Span::styled(format!("│{:^2}│", label), style));
    middle.push(Span::raw(format!("{:>2}", count_label(counts[1]))));
    bottom.push(Span::styled("└──┘", style));
    bottom.push(Span::raw(format!("{:>2}", count_label(counts[2]))));
    labels.push(Span::raw(format!("{:^6}", label)));
}

fn count_label(count: Option<u16>) -> String {
    count
        .map(|count| count.min(99).to_string())
        .unwrap_or_else(|| " ".to_owned())
}

fn dev_card_abbrev(card: UsableDevCard) -> &'static str {
    match card {
        UsableDevCard::Knight => "KN",
        UsableDevCard::YearOfPlenty => "YP",
        UsableDevCard::Monopoly => "M",
        UsableDevCard::RoadBuild => "RB",
    }
}

fn push_resource_values(
    spans: &mut Vec<Span<'static>>,
    resources: &ResourceCollection,
    format_count: impl Fn(u16) -> String,
) {
    for (idx, resource) in Resource::iter().into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(" "));
        }
        push_resource_value(spans, resource, format_count(resources[resource]));
    }
}

fn push_resource_value(
    spans: &mut Vec<Span<'static>>,
    resource: Resource,
    value: impl Into<String>,
) {
    let style = resource_style(resource);
    let name: &'static str = resource.into();
    spans.push(Span::styled(name, style));
    spans.push(Span::styled(":", style));
    spans.push(Span::styled(value.into(), style));
}

fn fullness_symbol(level: DeckFullnessLevel) -> &'static str {
    match level {
        DeckFullnessLevel::High => "???",
        DeckFullnessLevel::Medium => "??",
        DeckFullnessLevel::Low => "?",
        DeckFullnessLevel::Empty => "0",
    }
}

fn deck_fullness(count: u16) -> DeckFullnessLevel {
    DeckFullnessLevel::new(count).unwrap_or(DeckFullnessLevel::High)
}

fn resource_style(resource: Resource) -> Style {
    Style::default().fg(ratatui_color(FieldRenderer::resource_color(resource)))
}

fn player_style(player_id: PlayerId) -> Style {
    Style::default().fg(ratatui_color(FieldRenderer::player_color(player_id)))
}

#[cfg(test)]
mod tests {
    use catan_core::gameplay::primitives::{
        dev_card::DevCardData,
        resource::{Resource, ResourceCollection},
        trade::{BankTrade, BankTradeKind},
    };

    use super::{
        adjust_drop_selection, bank_trade_menu_lines, dev_card_lines, drop_personal_lines,
        resource_card_lines,
    };

    #[test]
    fn card_lines_render_resource_and_dev_counts() {
        let resources = ResourceCollection {
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
    fn drop_selection_is_bounded_by_available_resources() {
        let available = ResourceCollection {
            brick: 2,
            ..ResourceCollection::ZERO
        };
        let mut selected = ResourceCollection::ZERO;

        adjust_drop_selection(&available, &mut selected, Resource::Brick, 1);
        adjust_drop_selection(&available, &mut selected, Resource::Brick, 1);
        adjust_drop_selection(&available, &mut selected, Resource::Brick, 1);
        assert_eq!(selected.brick, 2);

        adjust_drop_selection(&available, &mut selected, Resource::Brick, -1);
        adjust_drop_selection(&available, &mut selected, Resource::Brick, -1);
        adjust_drop_selection(&available, &mut selected, Resource::Brick, -1);
        assert_eq!(selected.brick, 0);
    }

    #[test]
    fn drop_lines_show_selector_counts_and_total() {
        let resources = ResourceCollection {
            brick: 2,
            wood: 1,
            ..ResourceCollection::ZERO
        };
        let selected = ResourceCollection {
            brick: 1,
            ..ResourceCollection::ZERO
        };
        let dev_cards = DevCardData::default();
        let lines = drop_personal_lines(0, &resources, &dev_cards, &selected, 2, 0)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("drop 1 / 2 cards")));
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
        assert!(lines.iter().any(|line| line.contains("> 3:1 Wheat -> Ore")));
    }
}
