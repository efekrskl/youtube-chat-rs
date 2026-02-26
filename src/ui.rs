use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageKind {
    Text,
    Subscription,
}

#[derive(Debug, Clone)]
struct ChatMessage {
    timestamp: String,
    author: String,
    message: String,
    kind: MessageKind,
}

struct UiState {
    title: String,
    messages: Vec<ChatMessage>,
}

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

fn draw(frame: &mut Frame, app: &UiState) {
    let bg = Color::Rgb(35, 39, 65);
    let pane_border = Color::Rgb(137, 58, 255);
    let text = Color::Rgb(206, 212, 228);
    let muted = Color::Rgb(123, 131, 152);
    let sub_bg = Color::Rgb(28, 35, 58);

    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let items: Vec<ListItem> = app
        .messages
        .iter()
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

    let help = Paragraph::new(Line::from(vec![Span::styled(
        "[Ctrl+C] / [q]: quit",
        Style::default().fg(Color::Rgb(106, 112, 128)),
    )]))
    .style(Style::default().bg(bg))
    .wrap(Wrap { trim: true });

    frame.render_widget(chat, areas[0]);
    frame.render_widget(help, areas[1]);
}

// todo: remove
fn sample_messages() -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            timestamp: "14:36".to_string(),
            author: "EliQeener".to_string(),
            message: "hungy MYAAA".to_string(),
            kind: MessageKind::Text,
        },
        ChatMessage {
            timestamp: "14:36".to_string(),
            author: "misfitspadel46".to_string(),
            message: "big egg?".to_string(),
            kind: MessageKind::Text,
        },
        ChatMessage {
            timestamp: "14:36".to_string(),
            author: "Rapfan".to_string(),
            message: "Can you get all the pals and use them to wage war?".to_string(),
            kind: MessageKind::Text,
        },
        ChatMessage {
            timestamp: "14:36".to_string(),
            author: "Unain".to_string(),
            message: "You can work in your logging camp streamer".to_string(),
            kind: MessageKind::Text,
        },
        ChatMessage {
            timestamp: "14:36".to_string(),
            author: "EliQeener".to_string(),
            message: "subscribed for 1 months!".to_string(),
            kind: MessageKind::Subscription,
        },
        ChatMessage {
            timestamp: "14:36".to_string(),
            author: "AverageJonas".to_string(),
            message: "this game is so addicting".to_string(),
            kind: MessageKind::Text,
        },
        ChatMessage {
            timestamp: "14:37".to_string(),
            author: "Vanadin1".to_string(),
            message: "subscribed for 29 months: cute game cute chat".to_string(),
            kind: MessageKind::Subscription,
        },
        ChatMessage {
            timestamp: "14:37".to_string(),
            author: "EliQeener".to_string(),
            message: "GIGATON".to_string(),
            kind: MessageKind::Text,
        },
        ChatMessage {
            timestamp: "14:37".to_string(),
            author: "Son_Of_Ares".to_string(),
            message: "did the first boss respawn?".to_string(),
            kind: MessageKind::Text,
        },
        ChatMessage {
            timestamp: "14:37".to_string(),
            author: "Strayx".to_string(),
            message: "So how Ethical has seagull's expirence been".to_string(),
            kind: MessageKind::Text,
        },
    ]
}

pub fn run_ui() -> anyhow::Result<()> {
    let mut terminal = ratatui::init();

    let app = UiState {
        title: "SomeYtChannel".to_string(),
        messages: sample_messages(),
    };

    loop {
        terminal.draw(|frame| draw(frame, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let is_ctrl_c =
                key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
            let is_quit = matches!(key.code, KeyCode::Esc | KeyCode::Char('q'));

            if is_ctrl_c || is_quit {
                break;
            }
        }
    }

    Ok(())
}
