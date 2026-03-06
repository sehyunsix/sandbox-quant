use std::io::{self, Write};

use crossterm::cursor::{MoveToColumn, MoveToNextLine, RestorePosition, SavePosition};
use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::style::{Color, Print, PrintStyledContent, Stylize};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};

use crate::app::bootstrap::{AppBootstrap, BinanceMode};
use crate::app::cli::{complete_shell_input, parse_shell_input, shell_help_text, ShellInput};
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
                    render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    completion_index = 0;
                    render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                }
                KeyCode::Tab => {
                    let completions = current_completions(app, &buffer);
                    if completions.len() == 1 {
                        buffer = completions[0].clone();
                        completion_index = 0;
                        render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                    } else if !completions.is_empty() {
                        completion_index = (completion_index + 1) % completions.len();
                        buffer = completions[completion_index].clone();
                        render_shell(&mut stdout, app, &buffer, completion_index, &mut rendered_menu_lines)?;
                    }
                }
                KeyCode::Enter => {
                    println!();
                    let line = buffer.clone();
                    buffer.clear();
                    completion_index = 0;
                    rendered_menu_lines = 0;
                    match parse_shell_input(&line) {
                        Ok(ShellInput::Empty) => {}
                        Ok(ShellInput::Help) => println!("{}", shell_help_text()),
                        Ok(ShellInput::Exit) => break,
                        Ok(ShellInput::Mode(mode)) => {
                            match app.switch_mode(mode) {
                                Ok(()) => println!(
                                    "{} {}",
                                    "mode switched to".dark_grey(),
                                    mode_name(mode).with(mode_color(mode)).bold()
                                ),
                                Err(error) => print_error(error),
                            }
                        }
                        Ok(ShellInput::Command(command)) => {
                            let rendered_command = command.clone();
                            match runtime.run(app, command) {
                                Ok(()) => print_command_output(
                                    &rendered_command,
                                    &app.portfolio_store,
                                    &app.event_log,
                                ),
                                Err(error) => print_error(error),
                            }
                        }
                        Err(error) => print_error(error),
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
    render_prompt(stdout, app, buffer)?;
    execute!(stdout, SavePosition)?;

    let completions = current_completions(app, buffer);
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

pub fn format_completion_line(completions: &[String], selected: usize) -> String {
    completions
        .iter()
        .enumerate()
        .map(|(index, item)| {
            if index == selected {
                format!("[{item}]")
            } else {
                item.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn print_completion_menu(
    stdout: &mut io::Stdout,
    completions: &[String],
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
                PrintStyledContent(item.as_str().black().on_white()),
            )?;
        } else {
            execute!(
                stdout,
                Print("  "),
                PrintStyledContent(item.as_str().dark_grey()),
            )?;
        }
    }
    Ok(completions.len() + 1)
}

fn print_command_output(
    command: &crate::app::commands::AppCommand,
    store: &crate::portfolio::store::PortfolioStateStore,
    event_log: &crate::storage::event_log::EventLog,
) {
    println!("{}", render_command_output(command, store, event_log).cyan());
}

fn print_error(error: impl std::fmt::Display) {
    println!("{} {}", "error:".red().bold(), error.to_string().red());
}

fn current_completions(app: &AppBootstrap<BinanceExchange>, buffer: &str) -> Vec<String> {
    let instruments = app
        .portfolio_store
        .snapshot
        .positions
        .keys()
        .map(|instrument| instrument.0.clone())
        .collect::<Vec<_>>();
    complete_shell_input(buffer, &instruments)
}

fn should_show_completion_menu(buffer: &str, completions: &[String]) -> bool {
    buffer.trim_start().starts_with('/') && !completions.is_empty()
}
