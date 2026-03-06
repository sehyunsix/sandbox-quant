use sandbox_quant::app::bootstrap::AppBootstrap;
use sandbox_quant::error::exchange_error::ExchangeError;
use sandbox_quant::portfolio::store::PortfolioStateStore;

#[test]
fn app_bootstrap_from_env_requires_api_key() {
    unsafe {
        std::env::remove_var("BINANCE_API_KEY");
        std::env::remove_var("BINANCE_SECRET_KEY");
    }

    let error = match AppBootstrap::from_env(PortfolioStateStore::default()) {
        Ok(_) => panic!("missing configuration should fail"),
        Err(error) => error,
    };

    assert_eq!(error, ExchangeError::MissingConfiguration("BINANCE_API_KEY"));
}
