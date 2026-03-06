use hmac::{Hmac, Mac};
use sha2::Sha256;
use url::form_urlencoded::Serializer;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Default, Clone)]
pub struct BinanceAuth {
    api_key: String,
    secret_key: String,
}

impl BinanceAuth {
    pub fn new(api_key: impl Into<String>, secret_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            secret_key: secret_key.into(),
        }
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Creates a Binance-compatible signed query string.
    ///
    /// Example:
    /// - params: `symbol=BTCUSDT`, `side=BUY`
    /// - output: `symbol=BTCUSDT&side=BUY&signature=...`
    pub fn signed_query(&self, params: &[(&str, String)]) -> String {
        let mut serializer = Serializer::new(String::new());
        for (key, value) in params {
            serializer.append_pair(key, value);
        }
        let query = serializer.finish();
        let mut mac =
            HmacSha256::new_from_slice(self.secret_key.as_bytes()).expect("valid hmac key");
        mac.update(query.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        format!("{query}&signature={signature}")
    }
}
