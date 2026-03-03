use crate::app::event::{ChatMessage, MessageKind};
use crate::app::state::{AppState, CachedMessageLayout};
use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::{
    cursor::{MoveTo, RestorePosition, SavePosition},
    queue,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use std::io::{Write, stdout};
use std::sync::Arc;

const COLOR_BG: Color = Color::Rgb(35, 39, 65);
const COLOR_BORDER: Color = Color::Rgb(186, 104, 255);
const COLOR_TEXT: Color = Color::Rgb(206, 212, 228);
const COLOR_TEXT_MUTED: Color = Color::Rgb(123, 131, 152);
const COLOR_SUB_BG: Color = Color::Rgb(28, 35, 58);
const AVATAR_WIDTH: usize = 2;
const AVATAR_GAP: usize = 1;

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

#[derive(Clone, PartialEq, Eq)]
pub struct AvatarOverlay {
    pub x: u16,
    pub y: u16,
    pub image: Arc<String>,
}

fn avatar_placeholder(m: &ChatMessage) -> Span<'static> {
    let initial = m.author.chars().next().unwrap_or(' ');
    Span::styled(
        format!("{initial:<width$}", width = AVATAR_WIDTH),
        Style::default()
            .fg(Color::Rgb(18, 19, 32))
            .bg(nick_color(&m.author))
            .add_modifier(Modifier::BOLD),
    )
}

fn build_original_line(text: String, m: &ChatMessage) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        avatar_placeholder(m),
        Span::raw(" ".repeat(AVATAR_GAP)),
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

fn build_lines(m: &ChatMessage, layout: &CachedMessageLayout) -> Vec<ListItem<'static>> {
    match m.kind {
        MessageKind::Text => {
            let mut lines = Vec::with_capacity(layout.body_lines.len().max(1));
            let first_line = layout
                .body_lines
                .first()
                .cloned()
                .unwrap_or_else(|| m.message.clone());

            lines.push(build_original_line(first_line, m));

            let prefix = format!("[{}] {}: ", m.timestamp, m.author);
            let indent = " ".repeat(AVATAR_WIDTH + AVATAR_GAP + prefix.chars().count());

            for part in layout.body_lines.iter().skip(1) {
                lines.push(ListItem::new(Line::from(vec![
                    Span::styled(indent.clone(), Style::default().fg(COLOR_TEXT)),
                    Span::styled(part.clone(), Style::default().fg(COLOR_TEXT)),
                ])));
            }

            lines
        }
        MessageKind::Subscription => vec![ListItem::new(Line::from(vec![
            avatar_placeholder(m),
            Span::raw(" ".repeat(AVATAR_GAP)),
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
        ]))],
    }
}

fn build_title(app: &AppState) -> Line<'static> {
    Line::from(vec![
        Span::styled("[ ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("Channel", Style::default().fg(COLOR_BORDER)),
        Span::styled(": ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(app.title.clone(), Style::default().fg(COLOR_TEXT)),
        Span::styled(" ] - [ ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("Viewers", Style::default().fg(COLOR_BORDER)),
        Span::styled(": ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(app.stats.viewer_count.to_string(), Style::default().fg(COLOR_TEXT)),
        Span::styled(" ]", Style::default().fg(COLOR_TEXT_MUTED)),
    ])
}

fn collect_rows_and_overlays(
    app: &AppState,
    visible_rows: usize,
    inner: Rect,
) -> (Vec<ListItem<'static>>, Vec<AvatarOverlay>) {
    let total_rows = app.total_rows;
    let max_scroll = total_rows.saturating_sub(visible_rows);
    let scroll = app.scroll_state.scroll_offset.min(max_scroll);
    let end = total_rows.saturating_sub(scroll);
    let start = end.saturating_sub(visible_rows);

    let mut items = Vec::new();
    let mut overlays = Vec::new();
    let mut row_cursor = 0usize;

    for (message, layout) in app.messages.iter().zip(app.layouts.iter()) {
        let rows = build_lines(message, layout);
        let row_count = rows.len().max(1);
        let message_start = row_cursor;
        let message_end = row_cursor + row_count;

        if message_end <= start {
            row_cursor = message_end;
            continue;
        }

        if message_start >= end {
            break;
        }

        if let Some(avatar) = &message.avatar {
            if (start..end).contains(&message_start) {
                overlays.push(AvatarOverlay {
                    x: inner.x,
                    y: inner.y + (message_start - start) as u16,
                    image: Arc::clone(avatar),
                });
            }
        }

        let visible_start = start.saturating_sub(message_start);
        let visible_end = end.saturating_sub(message_start).min(row_count);
        items.extend(
            rows.into_iter()
                .skip(visible_start)
                .take(visible_end.saturating_sub(visible_start)),
        );

        row_cursor = message_end;
    }

    (items, overlays)
}

pub fn draw_with_overlays(frame: &mut Frame, app: &AppState) -> Vec<AvatarOverlay> {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let visible_rows = areas[0].height.saturating_sub(2) as usize;
    let inner = Rect::new(
        areas[0].x.saturating_add(1),
        areas[0].y.saturating_add(1),
        areas[0].width.saturating_sub(2),
        areas[0].height.saturating_sub(2),
    );
    let (items, overlays) = collect_rows_and_overlays(app, visible_rows, inner);

    let chat = List::new(items)
        .block(
            Block::default()
                .title(build_title(app))
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

    overlays
}

pub fn draw_avatar_overlays(overlays: &[AvatarOverlay], avatar_pixels: (u16, u16)) -> Result<()> {
    let mut out = stdout();

    for overlay in overlays {
        queue!(out, SavePosition, MoveTo(overlay.x, overlay.y))?;
        write!(
            out,
            "\x1b]1337;File=inline=1;preserveAspectRatio=0;doNotMoveCursor=1;width={}px;height={}px:{}\x07",
            avatar_pixels.0,
            avatar_pixels.1,
            overlay.image.as_str()
        )?;
        queue!(out, RestorePosition)?;
    }

    out.flush()?;
    Ok(())
}
