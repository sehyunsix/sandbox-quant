use sandbox_quant::app::bootstrap::AppBootstrap;
use sandbox_quant::app::commands::AppCommand;
use sandbox_quant::app::runtime::AppRuntime;
use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::market::Market;
use sandbox_quant::error::exchange_error::ExchangeError;
use sandbox_quant::exchange::facade::ExchangeFacade;
use sandbox_quant::exchange::symbol_rules::SymbolRules;
use sandbox_quant::exchange::types::{
    AuthoritativeSnapshot, CloseOrderAccepted, CloseOrderRequest, SubmitOrderAccepted,
};
use sandbox_quant::portfolio::store::PortfolioStateStore;

#[derive(Debug, Default)]
struct AuthFailExchange;

impl ExchangeFacade for AuthFailExchange {
    type Error = ExchangeError;

    fn load_authoritative_snapshot(&self) -> Result<AuthoritativeSnapshot, Self::Error> {
        Err(ExchangeError::AuthenticationFailed {
            status: 401,
            code: Some(-2015),
            endpoint: "/api/v3/account".to_string(),
            message: "Invalid API-key, IP, or permissions for action".to_string(),
        })
    }

    fn load_last_price(
        &self,
        _instrument: &Instrument,
        _market: Market,
    ) -> Result<f64, Self::Error> {
        Err(ExchangeError::UnsupportedMarketOperation)
    }

    fn load_symbol_rules(
        &self,
        _instrument: &Instrument,
        _market: Market,
    ) -> Result<SymbolRules, Self::Error> {
        Err(ExchangeError::UnsupportedMarketOperation)
    }

    fn submit_close_order(
        &self,
        _request: CloseOrderRequest,
    ) -> Result<CloseOrderAccepted, Self::Error> {
        Err(ExchangeError::UnsupportedMarketOperation)
    }

    fn submit_order(
        &self,
        _request: CloseOrderRequest,
    ) -> Result<SubmitOrderAccepted, Self::Error> {
        Err(ExchangeError::UnsupportedMarketOperation)
    }
}

#[test]
fn refresh_surfaces_detailed_authentication_failure_from_exchange() {
    let exchange = AuthFailExchange;
    let mut app = AppBootstrap::new(exchange, PortfolioStateStore::default());
    let mut runtime = AppRuntime::default();

    let error = runtime
        .run(&mut app, AppCommand::RefreshAuthoritativeState)
        .expect_err("refresh should surface exchange auth error");

    assert_eq!(
        error.to_string(),
        "exchange error: authentication failed: status=401 code=Some(-2015) endpoint=/api/v3/account message=Invalid API-key, IP, or permissions for action"
    );
}
