use crate::app::event::{ChatMessage, MessageKind};
use crate::app::state::AppState;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

const COLOR_BG: Color = Color::Rgb(35, 39, 65);
const COLOR_BORDER: Color = Color::Rgb(137, 58, 255);
const COLOR_TEXT: Color = Color::Rgb(206, 212, 228);
const COLOR_TEXT_MUTED: Color = Color::Rgb(123, 131, 152);
const COLOR_SUB_BG: Color = Color::Rgb(28, 35, 58);

fn nick_color(name: &str) -> Color {
    let palette = [
        Color::Cyan,
        Color::Green,
        Color::LightBlue,
        Color::LightRed,
        Color::LightMagenta,
        Color::Yellow,
        Color::LightCyan,
        Color::LightGreen,
    ];
    let hash = name.bytes().fold(0usize, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(b as usize)
    });
    palette[hash % palette.len()]
}

fn build_original_line(text: String, m: &ChatMessage) -> ListItem {
    ListItem::new(Line::from(vec![
        Span::styled(
            format!("[{}]", m.timestamp),
            Style::default().fg(COLOR_TEXT_MUTED),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{}:", m.author),
            Style::default()
                .fg(nick_color(&m.author))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(text, Style::default().fg(COLOR_TEXT)),
    ]))
}

fn build_lines(m: &ChatMessage, chat_width: usize) -> Vec<ListItem> {
    let prefix = format!("[{}] {}: ", m.timestamp, m.author);
    let prefix_len = prefix.chars().count();
    let body_width = chat_width.saturating_sub(prefix_len).max(1);
    let wrapped = textwrap::wrap(&m.message, body_width);

    let mut lines = Vec::with_capacity(wrapped.len().max(1));

    if wrapped.is_empty() {
        lines.push(build_original_line(m.message.clone(), m));

        return lines;
    }

    lines.push(build_original_line(wrapped[0].to_string(), m));
    let indent = " ".repeat(prefix_len);

    for part in wrapped.iter().skip(1) {
        lines.push(ListItem::new(Line::from(vec![
            Span::styled(indent.clone(), Style::default().fg(COLOR_TEXT)),
            Span::styled(part.to_string(), Style::default().fg(COLOR_TEXT)),
        ])));
    }

    lines
}

// todo: remove duplication?
fn row_count_for_message(m: &ChatMessage, chat_width: usize) -> usize {
    match m.kind {
        MessageKind::Text => {
            let prefix = format!("[{}] {}: ", m.timestamp, m.author);
            let prefix_len = prefix.chars().count();
            let body_width = chat_width.saturating_sub(prefix_len).max(1);
            let wrapped = textwrap::wrap(&m.message, body_width);
            wrapped.len().max(1)
        }
        MessageKind::Subscription => 1,
    }
}

pub fn max_scroll_for_viewport(app: &AppState, chat_width: usize, visible_rows: usize) -> usize {
    let total_rows = app
        .messages
        .iter()
        .map(|m| row_count_for_message(m, chat_width))
        .sum::<usize>();
    total_rows.saturating_sub(visible_rows)
}

pub fn draw(frame: &mut Frame, app: &AppState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let visible_rows = areas[0].height.saturating_sub(2) as usize;
    let chat_width = areas[0].width.saturating_sub(2) as usize;

    let all_rows: Vec<ListItem> = app
        .messages
        .iter()
        .flat_map(|m| match m.kind {
            MessageKind::Text => {
                let lines = build_lines(m, chat_width);

                lines
            }
            MessageKind::Subscription => {
                let line = Line::from(vec![
                    Span::styled(
                        format!(" {} ", m.author),
                        Style::default()
                            .fg(nick_color(&m.author))
                            .bg(COLOR_SUB_BG)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} ", m.message),
                        Style::default().fg(COLOR_TEXT).bg(COLOR_SUB_BG),
                    ),
                ]);

                vec![ListItem::new(line)]
            }
        })
        .collect();

    let total_rows = all_rows.len();
    let max_scroll = max_scroll_for_viewport(app, chat_width, visible_rows);
    let scroll = app.scroll_state.scroll_offset.min(max_scroll);
    let end = total_rows.saturating_sub(scroll);
    let start = end.saturating_sub(visible_rows);
    let items: Vec<ListItem> = all_rows
        .into_iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect();

    let chat = List::new(items)
        .block(
            Block::default()
                .title(format!(" {} - Viewers: {} ", app.title.clone(), app.stats.viewer_count))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_BORDER))
                .style(Style::default().bg(COLOR_BG)),
        )
        .style(Style::default().bg(COLOR_BG));

    let scroll_mode = if app.scroll_state.auto_scroll == true {
        "[FOLLOWING LIVE CHAT]"
    } else {
        "[FOLLOW DISABLED]"
    };

    let help = Paragraph::new(Line::from(vec![Span::styled(
        format!(
            "{} - [Up/Down/PgUp/PgDn/Home/End] scroll - [ESC/q] quit",
            scroll_mode
        ),
        Style::default().fg(Color::Rgb(106, 112, 128)),
    )]))
    .style(Style::default().bg(COLOR_BG))
    .wrap(Wrap { trim: true });

    frame.render_widget(chat, areas[0]);
    frame.render_widget(help, areas[1]);
}
