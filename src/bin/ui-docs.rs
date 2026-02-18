use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    sandbox_quant::ui_docs::run_cli(&args)
}
