use crate::app::event::{ChatMessage, MessageKind, StatusEvent};
use crate::app::state::AppState;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

const COLOR_BG: Color = Color::Rgb(35, 39, 65);
const COLOR_BORDER: Color = Color::Rgb(186, 104, 255);
const COLOR_TEXT: Color = Color::Rgb(206, 212, 228);
const COLOR_TEXT_MUTED: Color = Color::Rgb(123, 131, 152);
const COLOR_SUB_BG: Color = Color::Rgb(28, 35, 58);
const COLOR_SUPERCHAT_BG: Color = Color::Rgb(64, 44, 20);
const COLOR_SUPERCHAT_FG: Color = Color::Rgb(251, 191, 36);
const COLOR_OK: Color = Color::Rgb(110, 231, 183);
const COLOR_WARN: Color = Color::Rgb(251, 191, 36);
const COLOR_ERROR: Color = Color::Rgb(248, 113, 113);
const AVATAR_PLACEHOLDER_UNICODE: char = '\u{10EEEE}';

fn nick_color(name: &str) -> Color {
    let palette = [
        Color::Rgb(103, 232, 249),
        Color::Rgb(125, 211, 252),
        Color::Rgb(147, 197, 253),
        Color::Rgb(196, 181, 253),
        Color::Rgb(216, 180, 254),
        Color::Rgb(249, 168, 212),
        Color::Rgb(253, 164, 175),
        Color::Rgb(251, 146, 60),
        Color::Rgb(250, 204, 21),
        Color::Rgb(190, 242, 100),
        Color::Rgb(110, 231, 183),
        Color::Rgb(45, 212, 191),
        Color::Rgb(244, 114, 182),
        Color::Rgb(251, 191, 36),
        Color::Rgb(52, 211, 153),
        Color::Rgb(129, 140, 248),
    ];
    let hash = name.bytes().fold(0usize, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(b as usize)
    });
    palette[hash % palette.len()]
}

fn u32_to_color(value: u32) -> Color {
    Color::Rgb(
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    )
}

fn build_original_line(text: String, m: &ChatMessage) -> ListItem<'static> {
    let mut spans = Vec::with_capacity(7);

    if let Some(avatar) = &m.avatar {
        let placeholder: String =
            std::iter::repeat_n(AVATAR_PLACEHOLDER_UNICODE, avatar.cols as usize).collect();
        spans.push(Span::styled(
            placeholder,
            Style::default().fg(u32_to_color(avatar.id)),
        ));
    }

    spans.push(Span::styled(
        format!("[{}]", m.timestamp),
        Style::default().fg(COLOR_TEXT_MUTED),
    ));

    if let MessageKind::SuperChat { amount } = &m.kind {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" {amount} "),
            Style::default()
                .fg(COLOR_SUPERCHAT_FG)
                .bg(COLOR_SUPERCHAT_BG)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans.push(Span::raw(if m.is_member { " ⭐ " } else { " " }));
    spans.push(Span::styled(
        format!("{}:", m.author),
        Style::default()
            .fg(nick_color(&m.author))
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(text, Style::default().fg(COLOR_TEXT)));

    ListItem::new(Line::from(spans))
}

fn build_lines(m: &ChatMessage, chat_width: usize) -> Vec<ListItem<'static>> {
    let avatar_width = m.avatar.as_ref().map(|a| a.cols as usize).unwrap_or(0);
    let prefix = format!("[{}] {}: ", m.timestamp, m.author);
    let prefix_len = avatar_width + prefix.chars().count();
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
        MessageKind::Text | MessageKind::SuperChat { .. } => {
            let prefix = format!("[{}] {}: ", m.timestamp, m.author);
            let prefix_len = prefix.chars().count();
            let body_width = chat_width.saturating_sub(prefix_len).max(1);
            let wrapped = textwrap::wrap(&m.message, body_width);
            wrapped.len().max(1)
        }
        MessageKind::Membership | MessageKind::System => 1,
    }
}

fn build_banner(m: &ChatMessage, bg: Color) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(
            format!(" {} ", m.author),
            Style::default()
                .fg(nick_color(&m.author))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", m.message),
            Style::default().fg(COLOR_TEXT).bg(bg),
        ),
    ]))
}

fn build_message(m: &ChatMessage, chat_width: usize) -> Vec<ListItem<'static>> {
    match m.kind {
        MessageKind::Text | MessageKind::SuperChat { .. } => build_lines(m, chat_width),
        MessageKind::Membership => vec![build_banner(m, COLOR_SUB_BG)],
        MessageKind::System => vec![ListItem::new(Line::from(vec![
            Span::styled(
                format!("[{}] ", m.timestamp),
                Style::default().fg(COLOR_TEXT_MUTED),
            ),
            Span::styled(
                format!("{} {}", m.author, m.message),
                Style::default()
                    .fg(COLOR_TEXT_MUTED)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]))],
    }
}

fn build_title(app: &AppState) -> Line<'static> {
    let (status_text, status_color) = match app.connection.status {
        StatusEvent::Connecting => ("Connecting".to_string(), COLOR_WARN),
        StatusEvent::Connected => ("Connected".to_string(), COLOR_OK),
        StatusEvent::Reconnecting { attempt } => (format!("Reconnecting ({attempt})"), COLOR_WARN),
        StatusEvent::Disconnected => ("Disconnected".to_string(), COLOR_ERROR),
    };

    Line::from(vec![
        Span::styled("[ ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("Channel", Style::default().fg(COLOR_BORDER)),
        Span::styled(": ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(app.title.clone(), Style::default().fg(COLOR_TEXT)),
        Span::styled(" ] - [ ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("Viewers", Style::default().fg(COLOR_BORDER)),
        Span::styled(": ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(
            app.stats.viewer_count.to_string(),
            Style::default().fg(COLOR_TEXT),
        ),
        Span::styled(" ] - [ ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("Status", Style::default().fg(COLOR_BORDER)),
        Span::styled(": ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(status_text, Style::default().fg(status_color)),
        Span::styled(" ]", Style::default().fg(COLOR_TEXT_MUTED)),
    ])
}

pub fn max_scroll_for_viewport(app: &AppState, chat_width: usize, visible_rows: usize) -> usize {
    let total_rows = app
        .messages
        .iter()
        .map(|m| row_count_for_message(m, chat_width))
        .sum::<usize>();
    total_rows.saturating_sub(visible_rows)
}

fn build_footer(app: &AppState) -> Line<'static> {
    let scroll_mode = if app.scroll_state.auto_scroll {
        "[FOLLOWING LIVE CHAT]"
    } else {
        "[FOLLOW DISABLED]"
    };

    let mut spans = vec![Span::styled(
        format!("{scroll_mode} - [Up/Down/PgUp/PgDn/Home/End] scroll - [ESC/q] quit"),
        Style::default().fg(Color::Rgb(106, 112, 128)),
    )];

    // Surface backpressure instead of silently swallowing it.
    if app.dropped_messages > 0 {
        spans.push(Span::styled(
            format!(" - {} dropped", app.dropped_messages),
            Style::default().fg(COLOR_WARN),
        ));
    }

    if let Some(err) = &app.connection.last_error {
        spans.push(Span::styled(
            format!(" - {err}"),
            Style::default().fg(COLOR_ERROR),
        ));
    }

    Line::from(spans)
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
        .flat_map(|m| build_message(m, chat_width))
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
                .title(build_title(app))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_BORDER))
                .style(Style::default().bg(COLOR_BG)),
        )
        .style(Style::default().bg(COLOR_BG));

    let help = Paragraph::new(build_footer(app))
        .style(Style::default().bg(COLOR_BG))
        .wrap(Wrap { trim: true });

    frame.render_widget(chat, areas[0]);
    frame.render_widget(help, areas[1]);
}
