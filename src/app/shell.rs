use std::io::{self, Write};

use crossterm::cursor::{position, MoveToColumn, MoveToNextLine, MoveUp, RestorePosition, SavePosition};
use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::style::{Color, Print, PrintStyledContent, Stylize};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size, Clear, ClearType, ScrollUp};

use crate::app::bootstrap::{AppBootstrap, BinanceMode};
use crate::app::cli::{
    complete_shell_input_with_description, parse_shell_input, shell_help_text, ShellCompletion,
    ShellInput,
};
use crate::app::output::render_command_output;
use crate::app::runtime::AppRuntime;
use crate::exchange::binance::client::BinanceExchange;

pub fn run_shell(
    app: &mut AppBootstrap<BinanceExchange>,
    runtime: &mut AppRuntime,
) -> Result<(), Box<dyn std::error::Error>> {
    execute!(
        io::stdout(),
        PrintStyledContent("sandbox-quant".cyan().bold()),
        Print(" "),
        PrintStyledContent("interactive shell".dark_grey()),
        Print("\n"),
        PrintStyledContent("slash commands".dark_grey()),
        Print("\n"),
        Print(shell_help_text()),
        Print("\n")
    )?;

    enable_raw_mode()?;
    let result = loop_shell(app, runtime);
    disable_raw_mode()?;
    result
}

fn loop_shell(
    app: &mut AppBootstrap<BinanceExchange>,
    runtime: &mut AppRuntime,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout();
    let mut buffer = String::new();
    let mut completion_index = 0usize;
    let mut rendered_menu_lines = 0usize;
    let mut completion_query: Option<String> = None;
    render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;

    loop {
        if let Event::Key(key) = read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    println!();
                    break;
                }
                KeyCode::Char(ch) => {
                    buffer.push(ch);
                    completion_index = 0;
                    completion_query = Some(buffer.clone());
                    render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    completion_index = 0;
                    completion_query = Some(buffer.clone());
                    render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                }
                KeyCode::Tab => {
                    let query = completion_query.clone().unwrap_or_else(|| buffer.clone());
                    let completions = current_completions(app, &query);
                    if completions.len() == 1 {
                        buffer = completions[0].value.clone();
                        completion_index = 0;
                        completion_query = Some(query);
                        render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                    } else if !completions.is_empty() {
                        completion_index = (completion_index + 1) % completions.len();
                        buffer = completions[completion_index].value.clone();
                        completion_query = Some(query);
                        render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                    }
                }
                KeyCode::Up => {
                    let query = completion_query.clone().unwrap_or_else(|| buffer.clone());
                    let completions = current_completions(app, &query);
                    if !completions.is_empty() {
                        completion_index = previous_completion_index(completions.len(), completion_index);
                        buffer = completions[completion_index].value.clone();
                        completion_query = Some(query);
                        render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                    }
                }
                KeyCode::Down => {
                    let query = completion_query.clone().unwrap_or_else(|| buffer.clone());
                    let completions = current_completions(app, &query);
                    if !completions.is_empty() {
                        completion_index = next_completion_index(completions.len(), completion_index);
                        buffer = completions[completion_index].value.clone();
                        completion_query = Some(query);
                        render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                    }
                }
                KeyCode::Enter => {
                    clear_completion_menu(&mut stdout, rendered_menu_lines)?;
                    rendered_menu_lines = 0;
                    println!();
                    let line = buffer.clone();
                    buffer.clear();
                    completion_index = 0;
                    completion_query = None;
                    match parse_shell_input(&line) {
                        Ok(ShellInput::Empty) => {}
                        Ok(ShellInput::Help) => print_plain_block(shell_help_text())?,
                        Ok(ShellInput::Exit) => break,
                        Ok(ShellInput::Mode(mode)) => {
                            match app.switch_mode(mode) {
                                Ok(()) => print_mode_switched(&mut stdout, mode)?,
                                Err(error) => print_error(&mut stdout, error)?,
                            }
                        }
                        Ok(ShellInput::Command(command)) => {
                            let rendered_command = command.clone();
                            match runtime.run(app, command) {
                                Ok(()) => print_command_output(
                                    &mut stdout,
                                    &rendered_command,
                                    &app.portfolio_store,
                                    &app.event_log,
                                )?,
                                Err(error) => print_error(&mut stdout, error)?,
                            }
                        }
                        Err(error) => print_error(&mut stdout, error)?,
                    }
                    render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn render_prompt(
    stdout: &mut io::Stdout,
    app: &AppBootstrap<BinanceExchange>,
    buffer: &str,
) -> io::Result<()> {
    let mode = current_mode(app);
    let status = prompt_status(app);
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    execute!(
        stdout,
        PrintStyledContent("●".with(mode_color(mode))),
        Print(" "),
        PrintStyledContent(format!("[{}]", mode_name(mode)).with(mode_color(mode)).bold()),
        Print(" "),
        PrintStyledContent(status.dark_grey()),
        Print(" "),
        PrintStyledContent("›".cyan().bold()),
        Print(" "),
        Print(buffer)
    )?;
    stdout.flush()
}

fn render_shell(
    stdout: &mut io::Stdout,
    app: &AppBootstrap<BinanceExchange>,
    buffer: &str,
    completion_index: usize,
    rendered_menu_lines: &mut usize,
) -> io::Result<()> {
    let completions = current_completions(app, buffer);
    let expected_menu_lines = if should_show_completion_menu(buffer, &completions) {
        completions.len() + 1
    } else {
        0
    };
    let scrolled = ensure_vertical_space(stdout, expected_menu_lines)?;
    if scrolled > 0 {
        execute!(stdout, MoveUp(scrolled as u16))?;
    }
    render_prompt(stdout, app, buffer)?;
    execute!(stdout, SavePosition)?;
    let menu_lines = if should_show_completion_menu(buffer, &completions) {
        print_completion_menu(stdout, &completions, completion_index)?
    } else {
        0
    };

    let lines_to_clear = (*rendered_menu_lines).saturating_sub(menu_lines);
    for _ in 0..lines_to_clear {
        execute!(
            stdout,
            MoveToNextLine(1),
            MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        )?;
    }

    *rendered_menu_lines = menu_lines;
    execute!(stdout, RestorePosition)?;
    stdout.flush()
}

fn clear_completion_menu(stdout: &mut io::Stdout, rendered_menu_lines: usize) -> io::Result<()> {
    execute!(stdout, SavePosition)?;
    for _ in 0..rendered_menu_lines {
        execute!(
            stdout,
            MoveToNextLine(1),
            MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        )?;
    }
    execute!(stdout, RestorePosition)?;
    stdout.flush()
}

fn mode_name(mode: BinanceMode) -> &'static str {
    match mode {
        BinanceMode::Real => "real",
        BinanceMode::Demo => "demo",
    }
}

fn current_mode(app: &AppBootstrap<BinanceExchange>) -> BinanceMode {
    match app.exchange.transport_name() {
        "demo" => BinanceMode::Demo,
        _ => BinanceMode::Real,
    }
}

fn mode_color(mode: BinanceMode) -> Color {
    match mode {
        BinanceMode::Real => Color::Green,
        BinanceMode::Demo => Color::Yellow,
    }
}

fn prompt_status(app: &AppBootstrap<BinanceExchange>) -> String {
    format!(
        "[{}|{} pos]",
        staleness_label(app.portfolio_store.staleness),
        app.portfolio_store.snapshot.positions.len()
    )
}

fn staleness_label(staleness: crate::portfolio::staleness::StalenessState) -> &'static str {
    match staleness {
        crate::portfolio::staleness::StalenessState::Fresh => "fresh",
        crate::portfolio::staleness::StalenessState::MarketDataStale => "market-stale",
        crate::portfolio::staleness::StalenessState::AccountStateStale => "account-stale",
        crate::portfolio::staleness::StalenessState::ReconciliationStale => "reconcile-stale",
    }
}

pub fn format_completion_line(completions: &[ShellCompletion], selected: usize) -> String {
    completions
        .iter()
        .enumerate()
        .map(|(index, item)| {
            if index == selected {
                format!("[{}]", item.value)
            } else {
                item.value.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn print_completion_menu(
    stdout: &mut io::Stdout,
    completions: &[ShellCompletion],
    selected: usize,
) -> io::Result<usize> {
    execute!(
        stdout,
        MoveToNextLine(1),
        MoveToColumn(0),
        Clear(ClearType::CurrentLine),
        PrintStyledContent("completions".dark_grey()),
    )?;

    for (index, item) in completions.iter().enumerate() {
        execute!(stdout, MoveToNextLine(1), MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        if index == selected {
            execute!(
                stdout,
                PrintStyledContent(">".cyan().bold()),
                Print(" "),
                PrintStyledContent(item.value.as_str().black().on_white()),
                Print("  "),
                PrintStyledContent(item.description.as_str().dark_grey()),
            )?;
        } else {
            execute!(
                stdout,
                Print("  "),
                PrintStyledContent(item.value.as_str().dark_grey()),
                Print("  "),
                PrintStyledContent(item.description.as_str().dark_grey()),
            )?;
        }
    }
    Ok(completions.len() + 1)
}

fn print_command_output(
    stdout: &mut io::Stdout,
    command: &crate::app::commands::AppCommand,
    store: &crate::portfolio::store::PortfolioStateStore,
    event_log: &crate::storage::event_log::EventLog,
) -> io::Result<()> {
    begin_output_block(stdout)?;
    writeln!(stdout, "{}", render_command_output(command, store, event_log).cyan())
}

fn print_error(stdout: &mut io::Stdout, error: impl std::fmt::Display) -> io::Result<()> {
    begin_output_block(stdout)?;
    writeln!(stdout, "{} {}", "error:".red().bold(), error.to_string().red())
}

fn current_completions(
    app: &AppBootstrap<BinanceExchange>,
    buffer: &str,
) -> Vec<ShellCompletion> {
    let instruments = app
        .portfolio_store
        .snapshot
        .positions
        .keys()
        .map(|instrument| instrument.0.clone())
        .collect::<Vec<_>>();
    complete_shell_input_with_description(buffer, &instruments)
}

fn should_show_completion_menu(buffer: &str, completions: &[ShellCompletion]) -> bool {
    buffer.trim_start().starts_with('/') && !completions.is_empty()
}

fn begin_output_block(stdout: &mut io::Stdout) -> io::Result<()> {
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    Ok(())
}

fn print_plain_block(text: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    begin_output_block(&mut stdout)?;
    writeln!(stdout, "{text}")
}

fn print_mode_switched(stdout: &mut io::Stdout, mode: BinanceMode) -> io::Result<()> {
    begin_output_block(stdout)?;
    writeln!(
        stdout,
        "{} {}",
        "mode switched to".dark_grey(),
        mode_name(mode).with(mode_color(mode)).bold()
    )
}

fn ensure_vertical_space(stdout: &mut io::Stdout, lines_needed: usize) -> io::Result<usize> {
    if lines_needed == 0 {
        return Ok(0);
    }

    let (_, row) = position()?;
    let (_, height) = size()?;
    let overflow = scroll_lines_needed(row, height, lines_needed);
    if overflow > 0 {
        execute!(stdout, ScrollUp(overflow as u16))?;
    }
    Ok(overflow)
}

pub fn scroll_lines_needed(current_row: u16, terminal_height: u16, lines_needed: usize) -> usize {
    if terminal_height == 0 {
        return 0;
    }

    let last_row = terminal_height.saturating_sub(1) as usize;
    let current_row = current_row as usize;
    current_row.saturating_add(lines_needed).saturating_sub(last_row)
}

pub fn next_completion_index(len: usize, current: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1) % len
    }
}

pub fn previous_completion_index(len: usize, current: usize) -> usize {
    if len == 0 {
        0
    } else if current == 0 {
        len - 1
    } else {
        current - 1
    }
}
