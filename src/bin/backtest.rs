use anyhow::{Context, Result};

use sandbox_quant::backtest::{
    parse_backtest_args, parse_candle_csv, print_backtest_usage, run_walk_forward_backtest,
};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        println!("{}", print_backtest_usage());
        println!("\nExample:");
        println!(
            "  backtest --symbol BTCUSDT --bars data/demo_market_backlog.csv --order-usdt 100"
        );
        return Ok(());
    }

    let cfg = match parse_backtest_args(&args) {
        Ok(v) => v,
        Err(err) => {
            if err.to_string().contains("USAGE:") {
                println!("{}", err);
                return Ok(());
            }
            return Err(err).context("failed to parse backtest arguments");
        }
    };

    let feed = parse_candle_csv(&cfg.symbol, &cfg.bars_csv)?;
    let result = run_walk_forward_backtest(&cfg, &feed)?;

    println!("RunId: {}", result.run_id);
    println!("Symbol: {}", result.symbol);
    println!("Bars: {}", result.total_bars);
    println!(
        "Run period: {} - {}",
        result.run_started_ms, result.run_finished_ms
    );
    println!(
        "Folds: {}  Realized PnL: {:.6}  Fees: {:.6}  Trades: {}",
        result.folds.len(),
        result.metrics.realized_pnl_usdt,
        result.metrics.total_fees_usdt,
        result.metrics.trade_count
    );
    println!(
        "Win/Lose: {} / {}  MaxDD: {:.4}  Sharpe*: {:.4}  EndEq: {:.4}",
        result.metrics.win_count,
        result.metrics.lose_count,
        result.metrics.max_drawdown,
        result.metrics.sharpe_like,
        result.metrics.end_equity_usdt
    );

    println!();
    println!("Fold breakdown:");
    for fold in &result.folds {
        println!(
            "  Fold {}: train={} test={} realized={:.6} fees={:.6} dd={:.4} sharpe={:.4}",
            fold.fold,
            fold.train_bars,
            fold.test_bars,
            fold.metrics.realized_pnl_usdt,
            fold.metrics.total_fees_usdt,
            fold.metrics.max_drawdown,
            fold.metrics.sharpe_like
        );
    }

    Ok(())
}
