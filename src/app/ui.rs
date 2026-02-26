use crate::app::event::MessageKind;
use crate::app::state::AppState;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

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

pub fn draw(frame: &mut Frame, app: &AppState) {
    let bg = Color::Rgb(35, 39, 65);
    let pane_border = Color::Rgb(137, 58, 255);
    let text = Color::Rgb(206, 212, 228);
    let muted = Color::Rgb(123, 131, 152);
    let sub_bg = Color::Rgb(28, 35, 58);

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

    let items: Vec<ListItem> = app
        .messages
        .iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .map(|m| match m.kind {
            MessageKind::Text => {
                let line = Line::from(vec![
                    Span::styled(format!("[{}]", m.timestamp), Style::default().fg(muted)),
                    Span::raw(" "),
                    Span::styled(
                        format!("{}:", m.author),
                        Style::default()
                            .fg(nick_color(&m.author))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(m.message.clone(), Style::default().fg(text)),
                ]);

                ListItem::new(line)
            }
            MessageKind::Subscription => {
                let line = Line::from(vec![
                    Span::styled(
                        format!(" {} ", m.author),
                        Style::default()
                            .fg(nick_color(&m.author))
                            .bg(sub_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} ", m.message),
                        Style::default().fg(text).bg(sub_bg),
                    ),
                ]);

                ListItem::new(line)
            }
        })
        .collect();

    let chat = List::new(items)
        .block(
            Block::default()
                .title(app.title.clone())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(pane_border))
                .style(Style::default().bg(bg)),
        )
        .style(Style::default().bg(bg));

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
    .style(Style::default().bg(bg))
    .wrap(Wrap { trim: true });

    frame.render_widget(chat, areas[0]);
    frame.render_widget(help, areas[1]);
}
