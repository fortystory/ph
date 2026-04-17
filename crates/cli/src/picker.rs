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
    pub status: String,
}

pub enum Action {
    Select(usize),
    Add {
        title: String,
        priority: i32,
        status: String,
        projects: Vec<String>,
    },
    Edit {
        index: usize,
        title: String,
        priority: i32,
        status: String,
        projects: Vec<String>,
    },
    Delete(usize),
    Quit,
}

enum Mode {
    Normal,
    Search,
    Add,
    Edit,
}

struct FormState {
    focus: usize, // 0:title, 1:priority, 2:status, 3:projects
    title: String,
    title_before_edit: String,
    title_cursor: usize,
    editing_title: bool,
    priority: usize, // 0..=5
    status: usize,   // 0=todo, 1=done
    projects_selected: Vec<bool>,
    projects_cursor: usize,
    is_add: bool,
    error_msg: Option<String>,
}

impl FormState {
    fn new(projects: &[String]) -> Self {
        Self {
            focus: 0,
            title: String::new(),
            title_before_edit: String::new(),
            title_cursor: 0,
            editing_title: true,
            priority: 2,
            status: 0,
            projects_selected: vec![false; projects.len()],
            projects_cursor: 0,
            is_add: true,
            error_msg: None,
        }
    }

    fn from_item(item: &Item, projects: &[String]) -> Self {
        let item_projects: Vec<String> =
            item.projects.split_whitespace().map(|s| s.to_string()).collect();
        let selected: Vec<bool> = projects
            .iter()
            .map(|p| item_projects.contains(p))
            .collect();
        Self {
            focus: 0,
            title: item.title.clone(),
            title_before_edit: item.title.clone(),
            title_cursor: 0,
            editing_title: false,
            priority: item.priority.clamp(0, 5) as usize,
            status: if item.status == "done" { 1 } else { 0 },
            projects_selected: selected,
            projects_cursor: 0,
            is_add: false,
            error_msg: None,
        }
    }

    fn next_focus(&mut self) {
        self.editing_title = false;
        if self.is_add {
            self.focus = match self.focus {
                0 => 1,
                1 => 3,
                3 => 0,
                _ => 0,
            };
        } else {
            self.focus = (self.focus + 1) % 4;
        }
    }

    fn prev_focus(&mut self) {
        self.editing_title = false;
        if self.is_add {
            self.focus = match self.focus {
                0 => 3,
                1 => 0,
                3 => 1,
                _ => 0,
            };
        } else {
            self.focus = (self.focus + 3) % 4;
        }
    }

    fn enter_title_edit(&mut self) {
        self.title_before_edit = self.title.clone();
        self.editing_title = true;
        self.title_cursor = self.title.chars().count();
    }

    fn confirm_title_edit(&mut self) {
        self.editing_title = false;
    }

    fn cancel_title_edit(&mut self) {
        self.title = self.title_before_edit.clone();
        self.title_cursor = 0;
        self.editing_title = false;
    }

    fn byte_cursor(&self) -> usize {
        self.title
            .char_indices()
            .nth(self.title_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.title.len())
    }
}

struct App {
    items: Vec<Item>,
    filtered: Vec<usize>,
    table_state: TableState,
    search: String,
    mode: Mode,
    matcher: SkimMatcherV2,
    projects: Vec<String>,
    form: Option<FormState>,
    edit_index: Option<usize>,
    show_done: bool,
}

impl App {
    fn new(items: Vec<Item>, projects: Vec<String>) -> Self {
        let mut app = Self {
            items,
            filtered: Vec::new(),
            table_state: TableState::default().with_selected(0),
            search: String::new(),
            mode: Mode::Normal,
            matcher: SkimMatcherV2::default(),
            projects,
            form: None,
            edit_index: None,
            show_done: false,
        };
        app.refilter();
        app
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

    fn enter_add(&mut self) {
        self.mode = Mode::Add;
        self.form = Some(FormState::new(&self.projects));
        self.edit_index = None;
    }

    fn enter_edit(&mut self) {
        if let Some(item) = self.selected_item() {
            let form = FormState::from_item(item, &self.projects);
            let idx = item.index;
            self.mode = Mode::Edit;
            self.form = Some(form);
            self.edit_index = Some(idx);
        }
    }

    fn cancel_form(&mut self) {
        self.mode = Mode::Normal;
        self.form = None;
        self.edit_index = None;
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
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                if !self.show_done && item.status == "done" {
                    return None;
                }
                if self.search.is_empty() {
                    return Some(idx);
                }
                let text = format!(
                    "{} P{} {} {}",
                    item.title, item.priority, item.short_id, item.projects
                );
                self.matcher.fuzzy_match(&text, &self.search).map(|_| idx)
            })
            .collect();
        self.table_state.select(Some(0));
    }
}

pub fn pick(items: Vec<Item>, projects: Vec<String>) -> io::Result<Action> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(items, projects);
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
) -> io::Result<Action> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match app.mode {
                    Mode::Normal => match key.code {
                        KeyCode::Char('j') | KeyCode::Down => app.next(),
                        KeyCode::Char('k') | KeyCode::Up => app.previous(),
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(Action::Quit),
                        KeyCode::Char('/') => app.enter_search(),
                        KeyCode::Enter => {
                            if let Some(idx) = app.selected_original_index() {
                                return Ok(Action::Select(idx));
                            }
                        }
                        KeyCode::Char('a') => app.enter_add(),
                        KeyCode::Char('d') => {
                            if let Some(idx) = app.selected_original_index() {
                                return Ok(Action::Delete(idx));
                            }
                        }
                        KeyCode::Char('e') => app.enter_edit(),
                        KeyCode::Char('t') => {
                            app.show_done = !app.show_done;
                            app.refilter();
                        }
                        _ => {}
                    },
                    Mode::Search => match key.code {
                        KeyCode::Esc => app.exit_search(),
                        KeyCode::Char(c) => app.push_search(c),
                        KeyCode::Backspace => app.pop_search(),
                        KeyCode::Enter => {
                            if let Some(idx) = app.selected_original_index() {
                                return Ok(Action::Select(idx));
                            }
                        }
                        _ => {}
                    },
                    Mode::Add | Mode::Edit => {
                        if let Some(ref mut form) = app.form {
                            if form.editing_title {
                                match key.code {
                                    KeyCode::Esc => form.cancel_title_edit(),
                                    KeyCode::Enter => form.confirm_title_edit(),
                                    KeyCode::Left | KeyCode::Char('h') => {
                                        form.title_cursor = form.title_cursor.saturating_sub(1);
                                    }
                                    KeyCode::Right | KeyCode::Char('l') => {
                                        form.title_cursor = (form.title_cursor + 1)
                                            .min(form.title.chars().count());
                                    }
                                    KeyCode::Backspace => {
                                        if form.title_cursor > 0 {
                                            form.title_cursor -= 1;
                                            let byte_pos = form.byte_cursor();
                                            form.title.remove(byte_pos);
                                        }
                                    }
                                    KeyCode::Char(c) => {
                                        let byte_pos = form.byte_cursor();
                                        form.title.insert(byte_pos, c);
                                        form.title_cursor += 1;
                                    }
                                    _ => {}
                                }
                            } else {
                                form.error_msg = None;
                                match key.code {
                                    KeyCode::Esc => app.cancel_form(),
                                    KeyCode::Left | KeyCode::Char('h') => form.prev_focus(),
                                    KeyCode::Right | KeyCode::Char('l') => form.next_focus(),
                                    KeyCode::Enter => {
                                        let title = form.title.trim().to_string();
                                        if !title.is_empty() {
                                            let projects: Vec<String> = form
                                                .projects_selected
                                                .iter()
                                                .enumerate()
                                                .filter_map(|(i, &sel)| {
                                                    if sel {
                                                        Some(app.projects[i].clone())
                                                    } else {
                                                        None
                                                    }
                                                })
                                                .collect();
                                            if projects.is_empty() {
                                                form.error_msg = Some("至少选择一个关联项目".to_string());
                                            } else {
                                                let priority = form.priority as i32;
                                                let status = if form.status == 1 {
                                                    "done".to_string()
                                                } else {
                                                    "todo".to_string()
                                                };
                                                if matches!(app.mode, Mode::Add) {
                                                    return Ok(Action::Add {
                                                        title,
                                                        priority,
                                                        status,
                                                        projects,
                                                    });
                                                } else if let Some(idx) = app.edit_index {
                                                    return Ok(Action::Edit {
                                                        index: idx,
                                                        title,
                                                        priority,
                                                        status,
                                                        projects,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    _ => match form.focus {
                                        0 => match key.code {
                                            KeyCode::Char('e') => form.enter_title_edit(),
                                            KeyCode::Char('j') | KeyCode::Down => form.next_focus(),
                                            _ => {}
                                        },
                                        1 => match key.code {
                                            KeyCode::Char('j') | KeyCode::Down => {
                                                if form.priority < 5 {
                                                    form.priority += 1;
                                                }
                                            }
                                            KeyCode::Char('k') | KeyCode::Up => {
                                                if form.priority > 0 {
                                                    form.priority -= 1;
                                                }
                                            }
                                            _ => {}
                                        },
                                        2 => match key.code {
                                            KeyCode::Char('j') | KeyCode::Down => {
                                                form.status = 1;
                                            }
                                            KeyCode::Char('k') | KeyCode::Up => {
                                                form.status = 0;
                                            }
                                            _ => {}
                                        },
                                        3 => match key.code {
                                            KeyCode::Char('j') | KeyCode::Down => {
                                                if form.projects_cursor + 1 < app.projects.len() {
                                                    form.projects_cursor += 1;
                                                }
                                            }
                                            KeyCode::Char('k') | KeyCode::Up => {
                                                if form.projects_cursor > 0 {
                                                    form.projects_cursor -= 1;
                                                }
                                            }
                                            KeyCode::Char(' ') => {
                                                if let Some(sel) = form
                                                    .projects_selected
                                                    .get_mut(form.projects_cursor)
                                                {
                                                    *sel = !*sel;
                                                }
                                            }
                                            _ => {}
                                        },
                                        _ => {}
                                    },
                                }
                            }
                        }
                    }
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

    if matches!(app.mode, Mode::Add | Mode::Edit) {
        if let Some(ref form) = app.form {
            render_form(f, form, &app.projects, matches!(app.mode, Mode::Add));
        }
    }
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
            let p_style = priority_style(item.priority);
            let title_style = if item.status == "done" {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(Text::from(truncate_with_ellipsis(&item.title, 41))).style(title_style),
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

fn priority_style(p: i32) -> Style {
    match p {
        0 => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        1 => Style::default().fg(Color::Red),
        2 => Style::default().fg(Color::Yellow),
        3 => Style::default().fg(Color::Green),
        4 => Style::default().fg(Color::Cyan),
        _ => Style::default().fg(Color::Gray),
    }
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
        let p_style = priority_style(item.priority);
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
            let done_hint = if app.show_done { "t 隐藏完成" } else { "t 显示完成" };
            (
                format!(
                    "j/k 上下 | / 搜索 | 回车 确认 | a 添加 | e 编辑 | d 删除 | {} | q 退出 | {} 项",
                    done_hint,
                    app.filtered.len()
                ),
                Style::default().fg(Color::DarkGray),
            )
        }
        Mode::Search => (
            format!(
                "[搜索: {}] | Esc 清空 | 回车 确认 | {} 项",
                app.search,
                app.filtered.len()
            ),
            Style::default().fg(Color::DarkGray),
        ),
        Mode::Add | Mode::Edit => (String::new(), Style::default().fg(Color::DarkGray)),
    };
    let para = Paragraph::new(help.0).style(help.1);
    f.render_widget(para, area);
}

fn render_form(f: &mut Frame, form: &FormState, projects: &[String], is_add: bool) {
    let title_str = if is_add { "添加 Todo" } else { "编辑 Todo" };

    let area = centered_rect(70, 70, f.size());
    f.render_widget(Clear, area);

    let main_chunks = Layout::vertical([
        Constraint::Length(1), // 顶部标题
        Constraint::Min(1),    // 内容区
        Constraint::Length(1), // 底部帮助/错误
    ])
    .split(area);

    // 顶部标题
    let header = Paragraph::new(title_str)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(header, main_chunks[0]);

    // 内容区
    let content_chunks = if form.is_add {
        Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Min(1),
        ])
        .split(main_chunks[1])
    } else {
        Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Min(1),
        ])
        .split(main_chunks[1])
    };

    // Title
    let title_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if form.editing_title {
            Style::default().fg(Color::Cyan)
        } else if form.focus == 0 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title("标题");

    let title_text = if form.editing_title {
        let byte_cursor = form
            .title
            .char_indices()
            .nth(form.title_cursor)
            .map(|(i, _)| i)
            .unwrap_or(form.title.len());
        let before = &form.title[..byte_cursor];
        let after = &form.title[byte_cursor..];
        Text::from(Line::from(vec![
            Span::raw(before),
            Span::styled("|", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(after),
        ]))
    } else {
        Text::from(form.title.clone())
    };
    let title_para = Paragraph::new(title_text).block(title_block);
    f.render_widget(title_para, content_chunks[0]);

    // Priority
    let p_lines: Vec<Line> = (0..=5)
        .map(|i| {
            let s = format!("P{}", i);
            let style = if i == form.priority {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                priority_style(i as i32)
            };
            Line::from(Span::styled(s, style))
        })
        .collect();
    let p_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if form.focus == 1 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title("优先级");
    let p_para = Paragraph::new(Text::from(p_lines)).block(p_block);
    f.render_widget(p_para, content_chunks[1]);

    // Status (Edit only)
    if !form.is_add {
        let s_lines: Vec<Line> = vec![
            if form.status == 0 {
                Line::from(Span::styled(
                    " todo ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::raw(" todo "))
            },
            if form.status == 1 {
                Line::from(Span::styled(
                    " done ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::raw(" done "))
            },
        ];
        let s_block = Block::default()
            .borders(Borders::ALL)
            .border_style(if form.focus == 2 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            })
            .title("状态");
        let s_para = Paragraph::new(Text::from(s_lines)).block(s_block);
        f.render_widget(s_para, content_chunks[2]);
    }

    // Projects
    let proj_chunk = if form.is_add { content_chunks[2] } else { content_chunks[3] };
    let proj_lines: Vec<Line> = projects
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let marker = if form.projects_selected[i] { "[x]" } else { "[ ]" };
            let style = if i == form.projects_cursor && form.focus == 3 {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::styled(format!("{} ", marker), style),
                Span::styled(p.clone(), style),
            ])
        })
        .collect();
    let proj_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if form.focus == 3 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title("关联项目 (Space 切换)");
    let proj_para = Paragraph::new(Text::from(proj_lines)).block(proj_block);
    f.render_widget(proj_para, proj_chunk);

    // 底部帮助 / 错误提示
    let footer_span = if let Some(ref msg) = form.error_msg {
        Span::styled(msg.clone(), Style::default().fg(Color::Red))
    } else if form.editing_title {
        Span::styled(
            "h/l 移动光标 | Enter 确认 | Esc 撤销".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Span::styled(
            "e 编辑标题 | h/l 切换 | j/k 调整 | Space 选中 | Enter 提交 | Esc 取消".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    };
    let footer = Paragraph::new(Line::from(footer_span)).alignment(Alignment::Center);
    f.render_widget(footer, main_chunks[2]);
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
