use std::io::{self, Write};

use crossterm::cursor::MoveToColumn;
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
    render_prompt(&mut stdout, current_mode(app), &buffer)?;

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
                    render_prompt(&mut stdout, current_mode(app), &buffer)?;
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    completion_index = 0;
                    render_prompt(&mut stdout, current_mode(app), &buffer)?;
                }
                KeyCode::Tab => {
                    let instruments = app
                        .portfolio_store
                        .snapshot
                        .positions
                        .keys()
                        .map(|instrument| instrument.0.clone())
                        .collect::<Vec<_>>();
                    let completions = complete_shell_input(&buffer, &instruments);
                    if completions.len() == 1 {
                        buffer = completions[0].clone();
                        completion_index = 0;
                        render_prompt(&mut stdout, current_mode(app), &buffer)?;
                    } else if !completions.is_empty() {
                        completion_index = (completion_index + 1) % completions.len();
                        buffer = completions[completion_index].clone();
                        println!();
                        println!(
                            "{}",
                            format_completion_line(&completions, completion_index)
                        );
                        render_prompt(&mut stdout, current_mode(app), &buffer)?;
                    }
                }
                KeyCode::Enter => {
                    println!();
                    let line = buffer.clone();
                    buffer.clear();
                    completion_index = 0;
                    match parse_shell_input(&line)? {
                        ShellInput::Empty => {}
                        ShellInput::Help => println!("{}", shell_help_text()),
                        ShellInput::Exit => break,
                        ShellInput::Mode(mode) => {
                            app.switch_mode(mode)?;
                            println!(
                                "{} {}",
                                "mode switched to".dark_grey(),
                                mode_name(mode).with(mode_color(mode)).bold()
                            );
                        }
                        ShellInput::Command(command) => {
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
                    }
                    render_prompt(&mut stdout, current_mode(app), &buffer)?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn render_prompt(stdout: &mut io::Stdout, mode: BinanceMode, buffer: &str) -> io::Result<()> {
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    execute!(
        stdout,
        PrintStyledContent("●".with(mode_color(mode))),
        Print(" "),
        PrintStyledContent(format!("[{}]", mode_name(mode)).with(mode_color(mode)).bold()),
        Print(" "),
        PrintStyledContent("›".cyan().bold()),
        Print(" "),
        Print(buffer)
    )?;
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
