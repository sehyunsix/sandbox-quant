use std::io::{self, Write};

use sandbox_quant::app::cli::{parse_app_command, parse_shell_input, shell_help_text, ShellInput};
use sandbox_quant::app::bootstrap::AppBootstrap;
use sandbox_quant::app::output::render_command_output;
use sandbox_quant::app::runtime::AppRuntime;
use sandbox_quant::exchange::binance::client::BinanceExchange;
use sandbox_quant::portfolio::store::PortfolioStateStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut app = AppBootstrap::from_env(PortfolioStateStore::default())?;
    let mut runtime = AppRuntime::default();

    if args.is_empty() {
        run_shell(&mut app, &mut runtime)?;
    } else {
        let command = parse_app_command(&args)?;
        let rendered_command = command.clone();
        runtime.run(&mut app, command)?;
        println!(
            "{}",
            render_command_output(&rendered_command, &app.portfolio_store, &app.event_log)
        );
    }

    Ok(())
}

fn run_shell(
    app: &mut AppBootstrap<BinanceExchange>,
    runtime: &mut AppRuntime,
) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut line = String::new();

    println!("sandbox-quant interactive shell");
    println!("{}", shell_help_text());

    loop {
        print!("sq> ");
        io::stdout().flush()?;
        line.clear();

        if stdin.read_line(&mut line)? == 0 {
            break;
        }

        match parse_shell_input(&line)? {
            ShellInput::Empty => {}
            ShellInput::Help => println!("{}", shell_help_text()),
            ShellInput::Exit => break,
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
    }

    Ok(())
}
