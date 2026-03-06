use std::io::{self, Write};

use crossterm::cursor::MoveToColumn;
use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
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
    println!("sandbox-quant interactive shell");
    println!("{}", shell_help_text());

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
    render_prompt(&mut stdout, &buffer)?;

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
                    render_prompt(&mut stdout, &buffer)?;
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    render_prompt(&mut stdout, &buffer)?;
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
                        render_prompt(&mut stdout, &buffer)?;
                    } else if !completions.is_empty() {
                        println!();
                        println!("{}", completions.join("  "));
                        render_prompt(&mut stdout, &buffer)?;
                    }
                }
                KeyCode::Enter => {
                    println!();
                    let line = buffer.clone();
                    buffer.clear();
                    match parse_shell_input(&line)? {
                        ShellInput::Empty => {}
                        ShellInput::Help => println!("{}", shell_help_text()),
                        ShellInput::Exit => break,
                        ShellInput::Mode(mode) => {
                            app.switch_mode(mode)?;
                            println!("mode switched to {}", mode_name(mode));
                        }
                        ShellInput::Command(command) => {
                            let rendered_command = command.clone();
                            match runtime.run(app, command) {
                                Ok(()) => println!(
                                    "{}",
                                    render_command_output(
                                        &rendered_command,
                                        &app.portfolio_store,
                                        &app.event_log
                                    )
                                ),
                                Err(error) => println!("error: {error}"),
                            }
                        }
                    }
                    render_prompt(&mut stdout, &buffer)?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn render_prompt(stdout: &mut io::Stdout, buffer: &str) -> io::Result<()> {
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    write!(stdout, "sq> {buffer}")?;
    stdout.flush()
}

fn mode_name(mode: BinanceMode) -> &'static str {
    match mode {
        BinanceMode::Real => "real",
        BinanceMode::Demo => "demo",
    }
}
