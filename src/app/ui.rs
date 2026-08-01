use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::event::{ChatMessage, MessageKind, StatusEvent};
use crate::app::state::AppState;

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

/// Rows the chat list borders consume (top + bottom).
const CHAT_BORDER_ROWS: u16 = 2;
/// Columns the chat list borders consume (left + right).
const CHAT_BORDER_COLS: u16 = 2;

/// What `draw` measured, so callers do not have to recompute the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportMetrics {
    pub visible_rows: usize,
    pub max_scroll_rows: usize,
}

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

fn prefix_text(m: &ChatMessage) -> String {
    format!("[{}] {}: ", m.timestamp, m.author)
}

/// Terminal columns taken up before the message body starts.
///
/// Measured with `unicode-width`, not `chars().count()`: emoji and CJK
/// nicknames occupy two columns each, and counting them as one made the wrap
/// width wrong and the continuation lines misaligned.
fn prefix_width(m: &ChatMessage) -> usize {
    let avatar_width = m.avatar.as_ref().map(|a| a.cols as usize).unwrap_or(0);
    avatar_width + UnicodeWidthStr::width(prefix_text(m).as_str())
}

fn body_width(m: &ChatMessage, chat_width: usize) -> usize {
    chat_width.saturating_sub(prefix_width(m)).max(1)
}

/// How many terminal rows a message occupies.
///
/// This must agree exactly with what `build_lines` renders -- it previously
/// ignored the avatar column, so scroll bounds drifted for Kitty users and the
/// newest messages could be clipped.
pub fn row_count_for_message(m: &ChatMessage, chat_width: usize) -> usize {
    match m.kind {
        MessageKind::Text | MessageKind::SuperChat { .. } => {
            textwrap::wrap(&m.message, body_width(m, chat_width))
                .len()
                .max(1)
        }
        MessageKind::Membership | MessageKind::System => 1,
    }
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
    let wrapped = textwrap::wrap(&m.message, body_width(m, chat_width));

    let mut lines = Vec::with_capacity(wrapped.len().max(1));

    let Some(first) = wrapped.first() else {
        lines.push(build_original_line(m.message.clone(), m));
        return lines;
    };

    lines.push(build_original_line(first.to_string(), m));

    let indent = " ".repeat(prefix_width(m));
    for part in wrapped.iter().skip(1) {
        lines.push(ListItem::new(Line::from(vec![
            Span::raw(indent.clone()),
            Span::styled(part.to_string(), Style::default().fg(COLOR_TEXT)),
        ])));
    }

    lines
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

/// Rows each message occupies, in display order. Computed once per frame and
/// reused for both the scroll bounds and the visible-window slice.
pub fn row_counts(app: &AppState, chat_width: usize) -> Vec<usize> {
    app.messages
        .iter()
        .map(|m| row_count_for_message(m, chat_width))
        .collect()
}

fn max_scroll(total_rows: usize, visible_rows: usize) -> usize {
    total_rows.saturating_sub(visible_rows)
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

pub fn draw(frame: &mut Frame, app: &AppState) -> ViewportMetrics {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let visible_rows = areas[0].height.saturating_sub(CHAT_BORDER_ROWS) as usize;
    let chat_width = areas[0].width.saturating_sub(CHAT_BORDER_COLS) as usize;

    // One measurement pass; the old code wrapped every message three times per
    // frame (twice in `draw`, once more in the caller).
    let row_counts = row_counts(app, chat_width);
    let total_rows: usize = row_counts.iter().sum();

    let max_scroll = max_scroll(total_rows, visible_rows);
    let scroll = app.scroll_state.scroll_offset.min(max_scroll);
    let end = total_rows.saturating_sub(scroll);
    let start = end.saturating_sub(visible_rows);

    // Only build widgets for messages that intersect the viewport instead of
    // materialising all 500 and throwing most of them away.
    let mut items: Vec<ListItem> = Vec::with_capacity(visible_rows + 4);
    let mut row = 0usize;
    for (msg, rows) in app.messages.iter().zip(row_counts.iter()) {
        let msg_end = row + rows;
        if msg_end > start && row < end {
            let lines = build_message(msg, chat_width);
            for (i, line) in lines.into_iter().enumerate() {
                let line_row = row + i;
                if line_row >= start && line_row < end {
                    items.push(line);
                }
            }
        }
        row = msg_end;
        if row >= end {
            break;
        }
    }

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

    ViewportMetrics {
        visible_rows,
        max_scroll_rows: max_scroll,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::app::event::KittyAvatar;

    fn msg(text: &str) -> ChatMessage {
        ChatMessage {
            timestamp: "12:00".to_string(),
            author: "bob".to_string(),
            message: text.to_string(),
            kind: MessageKind::Text,
            avatar: None,
            avatar_url: None,
            is_member: false,
        }
    }

    fn avatar() -> Arc<KittyAvatar> {
        Arc::new(KittyAvatar {
            id: 1,
            cols: 2,
            width: 32,
            height: 32,
            path: "/tmp/a.rgba".to_string(),
        })
    }

    // Regression: the row count used for scroll bounds ignored the avatar
    // column while rendering accounted for it, so the two disagreed.
    #[test]
    fn row_count_matches_rendered_lines_with_avatar() {
        let mut m = msg(&"word ".repeat(40));
        m.avatar = Some(avatar());

        let width = 40;
        assert_eq!(
            row_count_for_message(&m, width),
            build_message(&m, width).len()
        );
    }

    #[test]
    fn row_count_matches_rendered_lines_without_avatar() {
        let m = msg(&"word ".repeat(40));
        let width = 40;
        assert_eq!(
            row_count_for_message(&m, width),
            build_message(&m, width).len()
        );
    }

    #[test]
    fn avatar_widens_the_prefix() {
        let plain = msg("hello");
        let mut with_avatar = msg("hello");
        with_avatar.avatar = Some(avatar());

        assert_eq!(prefix_width(&with_avatar), prefix_width(&plain) + 2);
    }

    #[test]
    fn wide_characters_count_as_two_columns() {
        let mut ascii = msg("hi");
        ascii.author = "abcd".to_string();
        let mut cjk = msg("hi");
        cjk.author = "日本語で".to_string();

        // Four characters either way, but the CJK nickname is twice as wide.
        assert_eq!(prefix_width(&cjk), prefix_width(&ascii) + 4);
    }

    #[test]
    fn turkish_characters_count_as_one_column() {
        let mut ascii = msg("hi");
        ascii.author = "ugsic".to_string();
        let mut turkish = msg("hi");
        turkish.author = "üğşiç".to_string();

        assert_eq!(prefix_width(&turkish), prefix_width(&ascii));
    }

    #[test]
    fn every_message_takes_at_least_one_row() {
        assert_eq!(row_count_for_message(&msg(""), 80), 1);
        // A pathologically narrow terminal must not divide by zero or panic.
        assert!(row_count_for_message(&msg("hello world"), 0) >= 1);
        assert!(row_count_for_message(&msg("hello world"), 1) >= 1);
    }

    #[test]
    fn banner_kinds_are_single_row() {
        let mut membership = msg(&"long ".repeat(50));
        membership.kind = MessageKind::Membership;
        assert_eq!(row_count_for_message(&membership, 20), 1);
        assert_eq!(build_message(&membership, 20).len(), 1);

        let mut system = msg(&"long ".repeat(50));
        system.kind = MessageKind::System;
        assert_eq!(row_count_for_message(&system, 20), 1);
        assert_eq!(build_message(&system, 20).len(), 1);
    }

    #[test]
    fn super_chat_wraps_like_text() {
        let mut sc = msg(&"word ".repeat(40));
        sc.kind = MessageKind::SuperChat {
            amount: "₺100".to_string(),
        };
        assert_eq!(row_count_for_message(&sc, 40), build_message(&sc, 40).len());
        assert!(row_count_for_message(&sc, 40) > 1);
    }

    fn total_rows(state: &AppState, chat_width: usize) -> usize {
        row_counts(state, chat_width).iter().sum()
    }

    #[test]
    fn max_scroll_is_zero_when_everything_fits() {
        let mut state = AppState::new("t".into());
        state.push_message(msg("one"));
        state.push_message(msg("two"));
        assert_eq!(max_scroll(total_rows(&state, 80), 40), 0);
    }

    #[test]
    fn max_scroll_counts_overflowing_rows() {
        let mut state = AppState::new("t".into());
        for _ in 0..10 {
            state.push_message(msg("x"));
        }
        assert_eq!(max_scroll(total_rows(&state, 80), 4), 6);
    }

    #[test]
    fn row_counts_include_wrapped_lines() {
        let mut state = AppState::new("t".into());
        state.push_message(msg("short"));
        state.push_message(msg(&"word ".repeat(40)));

        let counts = row_counts(&state, 40);
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0], 1);
        assert!(counts[1] > 1);
    }
}
