use sandbox_quant::config::Config;

struct EnvRestore {
    key: &'static str,
    prev: Option<String>,
}

impl EnvRestore {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, prev }
    }

    fn unset(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::remove_var(key);
        Self { key, prev }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        if let Some(v) = self.prev.as_ref() {
            std::env::set_var(self.key, v);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

#[test]
fn config_load_sanitizes_binance_credentials_from_env() {
    let _g1 = EnvRestore::set("BINANCE_API_KEY", "  \"spot-key-123\"  ");
    let _g2 = EnvRestore::set("BINANCE_API_SECRET", "  'spot-secret-456'  ");
    let _g3 = EnvRestore::set("BINANCE_FUTURES_API_KEY", "   ");
    let _g4 = EnvRestore::unset("BINANCE_FUTURES_API_SECRET");

    let cfg = Config::load().expect("config should load with sanitized credentials");
    assert_eq!(cfg.binance.api_key, "spot-key-123");
    assert_eq!(cfg.binance.api_secret, "spot-secret-456");
    assert_eq!(cfg.binance.futures_api_key, "spot-key-123");
    assert_eq!(cfg.binance.futures_api_secret, "spot-secret-456");
}
