use console::measure_text_width;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState};
use ratatui::Frame;
use ratatui::Terminal;
use std::io;

pub struct Item {
    pub title: String,
    pub priority: i32,
    pub short_id: String,
    pub projects: String,
    pub detail_lines: Vec<String>,
    pub index: usize,
}

enum Mode {
    Normal,
    Search,
}

struct App {
    items: Vec<Item>,
    filtered: Vec<usize>,
    table_state: TableState,
    search: String,
    mode: Mode,
    matcher: SkimMatcherV2,
}

impl App {
    fn new(items: Vec<Item>) -> Self {
        let count = items.len();
        let filtered: Vec<usize> = (0..count).collect();
        Self {
            items,
            filtered,
            table_state: TableState::default().with_selected(0),
            search: String::new(),
            mode: Mode::Normal,
            matcher: SkimMatcherV2::default(),
        }
    }

    fn selected_original_index(&self) -> Option<usize> {
        self.table_state
            .selected()
            .and_then(|i| self.filtered.get(i).copied())
            .map(|idx| self.items[idx].index)
    }

    fn selected_item(&self) -> Option<&Item> {
        self.table_state
            .selected()
            .and_then(|i| self.filtered.get(i).copied())
            .map(|idx| &self.items[idx])
    }

    fn next(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => {
                if i + 1 >= self.filtered.len() {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered.len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    fn enter_search(&mut self) {
        self.mode = Mode::Search;
    }

    fn exit_search(&mut self) {
        self.mode = Mode::Normal;
        self.search.clear();
        self.refilter();
    }

    fn push_search(&mut self, ch: char) {
        self.search.push(ch);
        self.refilter();
    }

    fn pop_search(&mut self) {
        self.search.pop();
        self.refilter();
    }

    fn refilter(&mut self) {
        if self.search.is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            self.filtered = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    let text = format!(
                        "{} P{} {} {}",
                        item.title, item.priority, item.short_id, item.projects
                    );
                    self.matcher.fuzzy_match(&text, &self.search).map(|_| idx)
                })
                .collect();
        }
        self.table_state.select(Some(0));
    }
}

pub fn pick(items: Vec<Item>) -> io::Result<Option<usize>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(items);
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<Option<usize>> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match app.mode {
                    Mode::Normal => match key.code {
                        KeyCode::Char('j') | KeyCode::Down => app.next(),
                        KeyCode::Char('k') | KeyCode::Up => app.previous(),
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                        KeyCode::Char('/') => app.enter_search(),
                        KeyCode::Enter => return Ok(app.selected_original_index()),
                        _ => {}
                    },
                    Mode::Search => match key.code {
                        KeyCode::Esc => app.exit_search(),
                        KeyCode::Char(c) => app.push_search(c),
                        KeyCode::Backspace => app.pop_search(),
                        KeyCode::Enter => return Ok(app.selected_original_index()),
                        _ => {}
                    },
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let content_height = 6 + 11 + 1 + 7 + 1;
    let area = centered_content(f.size(), content_height);

    let chunks = Layout::vertical([
        Constraint::Length(6),
        Constraint::Length(11),
        Constraint::Length(1),
        Constraint::Length(7),
        Constraint::Length(1),
    ])
    .split(area);

    render_logo(f, chunks[0]);
    render_table(f, app, chunks[1]);
    render_divider(f, chunks[2]);
    render_detail(f, app, chunks[3]);
    render_footer(f, app, chunks[4]);
}

fn centered_content(r: Rect, height: u16) -> Rect {
    let y = r.height.saturating_sub(height) / 2;
    let w = 92.min(r.width);
    let x = r.width.saturating_sub(w) / 2;
    Rect {
        x,
        y,
        width: w.min(r.width),
        height: height.min(r.height),
    }
}

fn render_logo(f: &mut Frame, area: Rect) {
    let text = Text::from(vec![
        Line::from("╔════════════════════════════════╗"),
        Line::from("║      P R O J E C T  H U B      ║"),
        Line::from("╚════════════════════════════════╝"),
        Line::from(""),
        Line::from(Span::styled("local-first ai pm", Style::default().fg(Color::Gray))),
        Line::from(""),
    ]);
    let para = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default());
    f.render_widget(para, area);
}

fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    let w = measure_text_width(s);
    if w <= max_width {
        return s.to_string();
    }
    if max_width <= 3 {
        return s.chars().take(max_width).collect();
    }
    let mut result = String::new();
    let mut width = 0;
    for ch in s.chars() {
        let ch_w = measure_text_width(&ch.to_string());
        if width + ch_w + 3 > max_width {
            result.push_str("...");
            break;
        }
        result.push(ch);
        width += ch_w;
    }
    result
}

fn render_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header = Row::new(vec![
        Cell::from("标题").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from(pad_left("优先级", 9)).style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("ID").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("项目").style(Style::default().add_modifier(Modifier::BOLD)),
    ])
    .height(1)
    .style(Style::default().fg(Color::White));

    let rows: Vec<Row> = app
        .filtered
        .iter()
        .map(|idx| {
            let item = &app.items[*idx];
            let p_style = match item.priority {
                0 => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                1 => Style::default().fg(Color::Red),
                2 => Style::default().fg(Color::Yellow),
                _ => Style::default().fg(Color::Green),
            };
            Row::new(vec![
                Cell::from(Text::from(truncate_with_ellipsis(&item.title, 41))),
                Cell::from(Text::from(pad_left(&format!("P{}", item.priority), 9))).style(p_style),
                Cell::from(Text::from(truncate_with_ellipsis(&item.short_id, 15))),
                Cell::from(Text::from(truncate_with_ellipsis(&item.projects, 26))),
            ])
            .height(1)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(50),
            Constraint::Length(9),
            Constraint::Length(15),
            Constraint::Percentage(32),
        ],
    )
    .header(header)
    .highlight_style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .style(Style::default());

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_divider(f: &mut Frame, area: Rect) {
    let line = "─".repeat(area.width as usize);
    let para = Paragraph::new(line).style(Style::default().fg(Color::DarkGray));
    f.render_widget(para, area);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    let key_style = Style::default().fg(Color::Gray);
    let max_val_width = (area.width as usize).saturating_sub(11);
    let text = if let Some(item) = app.selected_item() {
        let p_style = match item.priority {
            0 => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            1 => Style::default().fg(Color::Red),
            2 => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::Green),
        };
        let mut lines = vec![
            Line::from(vec![
                Span::styled(pad_right("标题", 10), key_style),
                Span::raw(" "),
                Span::raw(truncate_with_ellipsis(&item.title, max_val_width)),
            ]),
            Line::from(vec![
                Span::styled(pad_right("优先级", 10), key_style),
                Span::raw(" "),
                Span::styled(format!("P{}", item.priority), p_style),
            ]),
            Line::from(vec![
                Span::styled(pad_right("短 ID", 10), key_style),
                Span::raw(" "),
                Span::raw(truncate_with_ellipsis(&item.short_id, max_val_width)),
            ]),
            Line::from(vec![
                Span::styled(pad_right("项目", 10), key_style),
                Span::raw(" "),
                Span::raw(truncate_with_ellipsis(&item.projects, max_val_width)),
            ]),
        ];
        for line in &item.detail_lines {
            if let Some((k, v)) = line.split_once(':') {
                lines.push(Line::from(vec![
                    Span::styled(pad_right(k.trim(), 10), key_style),
                    Span::raw(" "),
                    Span::raw(truncate_with_ellipsis(v.trim(), max_val_width)),
                ]));
            } else {
                lines.push(Line::raw(truncate_with_ellipsis(line, area.width as usize)));
            }
        }
        Text::from(lines)
    } else {
        Text::from(truncate_with_ellipsis("未选择项目", area.width as usize))
    };

    let para = Paragraph::new(text).style(Style::default());
    f.render_widget(para, area);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let help = match app.mode {
        Mode::Normal => {
            format!(
                "j/k 上下 | / 搜索 | 回车 确认 | q 退出 | {} 项",
                app.filtered.len()
            )
        }
        Mode::Search => {
            format!(
                "[搜索: {}] | Esc 清空 | 回车 确认 | {} 项",
                app.search,
                app.filtered.len()
            )
        }
    };
    let para = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    f.render_widget(para, area);
}

fn pad_right(s: &str, width: usize) -> String {
    let w = measure_text_width(s);
    if w < width {
        format!("{}{}", s, " ".repeat(width - w))
    } else {
        s.to_string()
    }
}

fn pad_left(s: &str, width: usize) -> String {
    let w = measure_text_width(s);
    if w < width {
        format!("{}{}", " ".repeat(width - w), s)
    } else {
        s.to_string()
    }
}

// ---------- Confirm Popup ----------

struct ConfirmApp {
    msg: String,
    yes: bool,
}

pub fn confirm(msg: &str) -> io::Result<bool> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = ConfirmApp {
        msg: msg.to_string(),
        yes: false,
    };
    let res = run_confirm(&mut terminal, &mut app);

    disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run_confirm(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut ConfirmApp,
) -> io::Result<bool> {
    loop {
        terminal.draw(|f| confirm_ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                    KeyCode::Char('n') | KeyCode::Char('N') => return Ok(false),
                    KeyCode::Left | KeyCode::Char('h') => app.yes = true,
                    KeyCode::Right | KeyCode::Char('l') => app.yes = false,
                    KeyCode::Enter => return Ok(app.yes),
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(false),
                    _ => {}
                }
            }
        }
    }
}

fn confirm_ui(f: &mut Frame, app: &ConfirmApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let area = centered_rect(50, 20, f.size());
    f.render_widget(Clear, area);
    f.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);

    let text = Text::from(app.msg.clone());
    let para = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::White).bg(Color::Black));
    f.render_widget(para, chunks[0]);

    let yes_style = if app.yes {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let no_style = if app.yes {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD)
    };

    let buttons = Line::from(vec![
        Span::styled("  是  ", yes_style),
        Span::raw("   "),
        Span::styled("  否  ", no_style),
    ]);
    let btn_para = Paragraph::new(Text::from(vec![buttons]))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black));
    f.render_widget(btn_para, chunks[1]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
