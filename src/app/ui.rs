use crate::app::event::{ChatMessage, MessageKind};
use crate::app::state::AppState;
use crate::app::{AVATAR_GAP, AVATAR_HEIGHT, AVATAR_WIDTH};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui_image::Image;

const COLOR_BG: Color = Color::Rgb(35, 39, 65);
const COLOR_BORDER: Color = Color::Rgb(186, 104, 255);
const COLOR_TEXT: Color = Color::Rgb(206, 212, 228);
const COLOR_TEXT_MUTED: Color = Color::Rgb(123, 131, 152);
const COLOR_SUB_BG: Color = Color::Rgb(28, 35, 58);

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

fn build_original_line(text: String, m: &ChatMessage) -> Line<'_> {
    Line::from(vec![
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
    ])
}

fn build_lines(m: &ChatMessage, chat_width: usize) -> Vec<Line<'_>> {
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
        lines.push(Line::from(vec![
            Span::styled(indent.clone(), Style::default().fg(COLOR_TEXT)),
            Span::styled(part.to_string(), Style::default().fg(COLOR_TEXT)),
        ]));
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

fn build_title(app: &AppState) -> Line<'static> {
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

fn get_lines_by_kind(m: &ChatMessage, width: usize) -> Vec<Line<'_>> {
    match m.kind {
        MessageKind::Text => build_lines(m, width),
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

            vec![line]
        }
    }
}

fn get_visible_messages(
    app: &AppState,
    chat_width: usize,
    visible_rows: usize,
) -> Vec<(&ChatMessage, Vec<Line<'_>>)> {
    let mut remaining_scroll = app.scroll_state.scroll_offset;
    let mut used_rows = 0usize;
    let mut visible = Vec::new();

    for message in app.messages.iter().rev() {
        // todo: consts for avatar with + gap
        let lines = get_lines_by_kind(
            message,
            chat_width
                .saturating_sub((AVATAR_WIDTH + AVATAR_GAP) as usize)
                .max(1),
        );
        let height = row_count_for_message(message, chat_width);

        if remaining_scroll >= height {
            remaining_scroll -= height;
            continue;
        }

        if remaining_scroll > 0 {
            remaining_scroll = 0;
            continue;
        }

        if used_rows + height > visible_rows {
            break;
        }

        used_rows += height;
        visible.push((message, lines));
    }

    visible.reverse();

    visible
}

fn render_message(frame: &mut Frame, area: Rect, message: &ChatMessage, lines: &[Line]) {
    let [avatar_area, _gap_area, text_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(AVATAR_WIDTH),
            Constraint::Length(AVATAR_GAP),
            Constraint::Min(1),
        ])
        .areas(area);

    if let Some(avatar) = message.avatar.as_ref() {
        let image_height = avatar_area.height.min(AVATAR_HEIGHT);
        let centered_y = avatar_area.y + avatar_area.height.saturating_sub(image_height) / 2;
        let image_area = Rect::new(avatar_area.x, centered_y, avatar_area.width, image_height);
        frame.render_widget(Image::new(avatar), image_area);
    }

    frame.render_widget(
        Paragraph::new(lines.to_vec())
            .style(Style::default().bg(COLOR_BG))
            .wrap(Wrap { trim: false }),
        text_area,
    );
}

pub fn draw(frame: &mut Frame, app: &AppState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let visible_rows = areas[0].height.saturating_sub(2) as usize;
    let chat_width = areas[0].width.saturating_sub(2) as usize;

    let scroll_mode = if app.scroll_state.auto_scroll {
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

    let chat_block = Block::default()
        .title(build_title(app))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(COLOR_BORDER))
        .style(Style::default().bg(COLOR_BG));
    let inner = chat_block.inner(areas[0]);
    frame.render_widget(chat_block, areas[0]);

    let visible = get_visible_messages(app, chat_width, visible_rows);
    let used_rows = visible.iter().map(|(_, lines)| lines.len()).sum::<usize>();
    let mut y = inner
        .y
        .saturating_add(inner.height.saturating_sub(used_rows as u16));

    for (message, lines) in visible {
        let height = lines.len() as u16;
        let message_area = Rect::new(inner.x, y, inner.width, height);
        render_message(frame, message_area, message, &lines);
        y = y.saturating_add(height);
    }

    frame.render_widget(help, areas[1]);
}
