use sandbox_quant::app::bootstrap::AppBootstrap;
use sandbox_quant::app::cli::parse_app_command;
use sandbox_quant::app::output::render_command_output;
use sandbox_quant::app::runtime::AppRuntime;
use sandbox_quant::app::shell::run_shell;
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
            render_command_output(
                &rendered_command,
                &app.portfolio_store,
                &app.price_store,
                &app.event_log,
                &app.strategy_store,
                app.mode,
            )
        );
    }

    Ok(())
}
