use sandbox_quant::app::bootstrap::AppBootstrap;
use sandbox_quant::app::commands::AppCommand;
use sandbox_quant::app::runtime::AppRuntime;
use sandbox_quant::portfolio::store::PortfolioStateStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let command = parse_command(std::env::args().nth(1).as_deref())?;
    let mut app = AppBootstrap::from_env(PortfolioStateStore::default())?;
    let mut runtime = AppRuntime::default();

    runtime.run(&mut app, command)?;

    if let Some(last_event) = app.event_log.records.last() {
        println!("{}", last_event.kind);
    }

    Ok(())
}

fn parse_command(raw: Option<&str>) -> Result<AppCommand, String> {
    match raw.unwrap_or("refresh") {
        "refresh" => Ok(AppCommand::RefreshAuthoritativeState),
        other => Err(format!(
            "unsupported command: {other}. supported commands: refresh"
        )),
    }
}
