use sandbox_quant::error::exchange_error::ExchangeError;
use sandbox_quant::exchange::binance::auth::BinanceAuth;
use sandbox_quant::exchange::binance::client::map_binance_http_error;

#[test]
fn binance_auth_signs_query_with_hmac_sha256_suffix() {
    let auth = BinanceAuth::new("api-key", "secret-key");
    let signed = auth.signed_query(&[
        ("symbol", "BTCUSDT".to_string()),
        ("side", "BUY".to_string()),
    ]);

    assert!(signed.starts_with("symbol=BTCUSDT&side=BUY&signature="));
    assert_eq!(signed.len(), "symbol=BTCUSDT&side=BUY&signature=".len() + 64);
}

#[test]
fn binance_http_error_maps_timestamp_and_authentication_codes() {
    assert_eq!(
        map_binance_http_error(400, r#"{"code":-1021,"msg":"invalid timestamp"}"#, "/api/v3/account"),
        ExchangeError::InvalidTimestamp
    );
    assert_eq!(
        map_binance_http_error(401, r#"{"code":-2015,"msg":"invalid api-key"}"#, "/api/v3/account"),
        ExchangeError::AuthenticationFailed {
            status: 401,
            code: Some(-2015),
            endpoint: "/api/v3/account".to_string(),
            message: "invalid api-key".to_string(),
        }
    );
}

#[test]
fn binance_http_error_maps_rate_limit_and_remote_rejects() {
    assert_eq!(
        map_binance_http_error(429, r#"{"code":-1003,"msg":"too many requests"}"#, "/fapi/v1/order"),
        ExchangeError::RateLimited {
            status: 429,
            code: Some(-1003),
            endpoint: "/fapi/v1/order".to_string(),
            message: "too many requests".to_string(),
        }
    );
    assert_eq!(
        map_binance_http_error(400, r#"{"code":-2022,"msg":"reduce only rejected"}"#, "/fapi/v1/order"),
        ExchangeError::RemoteReject {
            code: -2022,
            message: "reduce only rejected".to_string(),
        }
    );
}
