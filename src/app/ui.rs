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

pub fn draw(frame: &mut Frame, app: &AppState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let visible_rows = areas[0].height.saturating_sub(2) as usize;
    let total = app.messages.len();
    let max_scroll = total.saturating_sub(visible_rows);
    let scroll = app.scroll_offset.min(max_scroll);
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(visible_rows);
    let chat_width = areas[0].width.saturating_sub(2) as usize;

    let items: Vec<ListItem> = app
        .messages
        .iter()
        .skip(start)
        .take(end.saturating_sub(start))
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

    let chat = List::new(items)
        .block(
            Block::default()
                .title(app.title.clone())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_BORDER))
                .style(Style::default().bg(COLOR_BG)),
        )
        .style(Style::default().bg(COLOR_BG));

    let scroll_mode = if app.auto_scroll == true {
        "[FOLLOWING LIVE CHAT]"
    } else {
        "[FOLLOW DISABLED]"
    };

    let help = Paragraph::new(Line::from(vec![Span::styled(
        format!(
            "{} - [Up/Down/PgUp/PgDn/Home/End] scroll - [Ctrl+C]/[q] quit",
            scroll_mode
        ),
        Style::default().fg(Color::Rgb(106, 112, 128)),
    )]))
    .style(Style::default().bg(COLOR_BG))
    .wrap(Wrap { trim: true });

    frame.render_widget(chat, areas[0]);
    frame.render_widget(help, areas[1]);
}
