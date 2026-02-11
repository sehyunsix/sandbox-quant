use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("config error: {0}")]
    Config(String),

    #[error("binance API error (code {code}): {msg}")]
    BinanceApi { code: i64, msg: String },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("order error: {0}")]
    Order(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
