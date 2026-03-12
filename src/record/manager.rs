use std::fs;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::app::bootstrap::BinanceMode;
use crate::dataset::types::{BacktestDatasetSummary, RecorderMetrics};
use crate::error::storage_error::StorageError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordManager {
    base_dir: PathBuf,
    spawn_enabled: bool,
}

impl Default for RecordManager {
    fn default() -> Self {
        Self::new("var")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRuntimeStatus {
    pub mode: BinanceMode,
    pub pid: Option<u32>,
    pub process_alive: bool,
    pub worker_alive: bool,
    pub state: String,
    pub desired_running: bool,
    pub status_stale: bool,
    pub heartbeat_age_sec: Option<i64>,
    pub binary_version: String,
    pub db_path: PathBuf,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub manual_symbols: Vec<String>,
    pub strategy_symbols: Vec<String>,
    pub watched_symbols: Vec<String>,
    pub metrics: RecorderMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordConfigFile {
    pub mode: String,
    pub desired_running: bool,
    pub manual_symbols: Vec<String>,
    pub strategy_symbols: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordStatusFile {
    pub mode: String,
    pub pid: u32,
    pub state: String,
    pub binary_version: String,
    pub db_path: String,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub manual_symbols: Vec<String>,
    pub strategy_symbols: Vec<String>,
    pub watched_symbols: Vec<String>,
    #[serde(default)]
    pub worker_alive: bool,
}

impl RecordManager {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            spawn_enabled: true,
        }
    }

    pub fn without_spawn(mut self) -> Self {
        self.spawn_enabled = false;
        self
    }

    pub fn start(
        &self,
        mode: BinanceMode,
        manual_symbols: Vec<String>,
        strategy_symbols: Vec<String>,
    ) -> Result<RecordRuntimeStatus, StorageError> {
        fs::create_dir_all(&self.base_dir).map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?;
        let config = RecordConfigFile {
            mode: format_mode(mode).to_string(),
            desired_running: true,
            manual_symbols,
            strategy_symbols,
            updated_at: Utc::now().to_rfc3339(),
        };
        self.write_config(mode, &config)?;

        if self.spawn_enabled {
            if !self.is_running(mode)? {
                self.spawn_recorder_process(mode)?;
            }
            self.wait_for_status(mode, Duration::from_secs(2))?;
        } else {
            crate::storage::market_data_store::MarketDataRecorder::init_schema_for_path(
                &self.db_path(mode),
            )?;
            self.write_status_file(
                mode,
                &RecordStatusFile {
                    mode: format_mode(mode).to_string(),
                    pid: 0,
                    state: "running".to_string(),
                    binary_version: env!("CARGO_PKG_VERSION").to_string(),
                    db_path: self.db_path(mode).display().to_string(),
                    started_at: Some(Utc::now().to_rfc3339()),
                    updated_at: Utc::now().to_rfc3339(),
                    manual_symbols: config.manual_symbols.clone(),
                    strategy_symbols: config.strategy_symbols.clone(),
                    watched_symbols: merge_symbols(
                        config.manual_symbols.clone(),
                        config.strategy_symbols.clone(),
                    ),
                    worker_alive: true,
                },
            )?;
        }

        self.status(mode)
    }

    pub fn sync_strategy_symbols(
        &self,
        mode: BinanceMode,
        strategy_symbols: Vec<String>,
    ) -> Result<(), StorageError> {
        let mut config = self.read_config(mode)?.unwrap_or_else(|| RecordConfigFile {
            mode: format_mode(mode).to_string(),
            desired_running: false,
            manual_symbols: Vec::new(),
            strategy_symbols: Vec::new(),
            updated_at: Utc::now().to_rfc3339(),
        });
        config.strategy_symbols = strategy_symbols;
        config.updated_at = Utc::now().to_rfc3339();
        self.write_config(mode, &config)
    }

    pub fn stop(&self, mode: BinanceMode) -> Result<RecordRuntimeStatus, StorageError> {
        let mut config = self.read_config(mode)?.unwrap_or_else(|| RecordConfigFile {
            mode: format_mode(mode).to_string(),
            desired_running: false,
            manual_symbols: Vec::new(),
            strategy_symbols: Vec::new(),
            updated_at: Utc::now().to_rfc3339(),
        });
        config.desired_running = false;
        config.updated_at = Utc::now().to_rfc3339();
        self.write_config(mode, &config)?;

        if let Some(status) = self.read_status_file(mode)? {
            if status.pid > 0 && is_process_alive(status.pid) {
                let _ = terminate_process(status.pid);
            }
        }
        if !self.spawn_enabled {
            self.write_status_file(
                mode,
                &RecordStatusFile {
                    mode: format_mode(mode).to_string(),
                    pid: 0,
                    state: "stopped".to_string(),
                    binary_version: env!("CARGO_PKG_VERSION").to_string(),
                    db_path: self.db_path(mode).display().to_string(),
                    started_at: None,
                    updated_at: Utc::now().to_rfc3339(),
                    manual_symbols: config.manual_symbols.clone(),
                    strategy_symbols: config.strategy_symbols.clone(),
                    watched_symbols: merge_symbols(
                        config.manual_symbols.clone(),
                        config.strategy_symbols.clone(),
                    ),
                    worker_alive: false,
                },
            )?;
        }

        thread::sleep(Duration::from_millis(200));
        self.status(mode)
    }

    pub fn status(&self, mode: BinanceMode) -> Result<RecordRuntimeStatus, StorageError> {
        let config = self.read_config(mode)?;
        let status = self.read_status_file(mode)?;
        let db_path = self.db_path(mode);
        let metrics =
            crate::storage::market_data_store::MarketDataRecorder::metrics_for_path(&db_path)
                .unwrap_or_default();
        let config_manual = config
            .as_ref()
            .map(|value| value.manual_symbols.clone())
            .unwrap_or_default();
        let config_strategy = config
            .as_ref()
            .map(|value| value.strategy_symbols.clone())
            .unwrap_or_default();
        let watched_symbols = merge_symbols(config_manual.clone(), config_strategy.clone());

        Ok(match status {
            Some(status) => {
                let updated_at = DateTime::parse_from_rfc3339(&status.updated_at)
                    .map(|value| value.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                let heartbeat_age_sec = (Utc::now() - updated_at).num_seconds();
                let process_alive = status.pid == 0
                    || is_process_alive(status.pid)
                    || (status.worker_alive && heartbeat_age_sec <= 5);
                RecordRuntimeStatus {
                    mode,
                    pid: Some(status.pid),
                    process_alive,
                    worker_alive: status.worker_alive,
                    state: if process_alive {
                        status.state
                    } else if status.state == "running" {
                        "stopped".to_string()
                    } else {
                        status.state
                    },
                    desired_running: config
                        .as_ref()
                        .map(|value| value.desired_running)
                        .unwrap_or(false),
                    status_stale: heartbeat_age_sec > 5,
                    heartbeat_age_sec: Some(heartbeat_age_sec),
                    binary_version: status.binary_version,
                    db_path,
                    started_at: status
                        .started_at
                        .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
                        .map(|value| value.with_timezone(&Utc)),
                    updated_at,
                    manual_symbols: config_manual,
                    strategy_symbols: config_strategy,
                    watched_symbols,
                    metrics,
                }
            }
            None => RecordRuntimeStatus {
                mode,
                pid: None,
                process_alive: false,
                worker_alive: false,
                state: "stopped".to_string(),
                desired_running: config
                    .as_ref()
                    .map(|value| value.desired_running)
                    .unwrap_or(false),
                status_stale: false,
                heartbeat_age_sec: None,
                binary_version: env!("CARGO_PKG_VERSION").to_string(),
                db_path,
                started_at: None,
                updated_at: Utc::now(),
                manual_symbols: config_manual,
                strategy_symbols: config_strategy,
                watched_symbols,
                metrics,
            },
        })
    }

    pub fn backtest_dataset_summary(
        &self,
        mode: BinanceMode,
        symbol: &str,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> Result<BacktestDatasetSummary, StorageError> {
        crate::storage::market_data_store::MarketDataRecorder::backtest_summary_for_path(
            &self.db_path(mode),
            mode,
            symbol,
            from,
            to,
        )
    }

    pub fn current_config(
        &self,
        mode: BinanceMode,
    ) -> Result<Option<(Vec<String>, Vec<String>)>, StorageError> {
        Ok(self
            .read_config(mode)?
            .map(|config| (config.manual_symbols, config.strategy_symbols)))
    }

    pub fn load_config_file(
        &self,
        mode: BinanceMode,
    ) -> Result<Option<RecordConfigFile>, StorageError> {
        self.read_config(mode)
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    fn spawn_recorder_process(&self, mode: BinanceMode) -> Result<(), StorageError> {
        let binary_path = self.resolve_recorder_binary_path()?;
        let _ = fs::remove_file(self.status_path(mode));
        let log_path = self
            .base_dir
            .join(format!("record-{}.log", format_mode(mode)));
        let log_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
        let mut command = Command::new(binary_path);
        command
            .arg("run")
            .arg("--mode")
            .arg(format_mode(mode))
            .arg("--base-dir")
            .arg(self.base_dir.display().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file.try_clone().map_err(|error| {
                StorageError::WriteFailedWithContext {
                    message: error.to_string(),
                }
            })?))
            .stderr(Stdio::from(log_file));
        #[cfg(unix)]
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command
            .spawn()
            .map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
        Ok(())
    }

    fn wait_for_status(&self, mode: BinanceMode, timeout: Duration) -> Result<(), StorageError> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if self.read_status_file(mode)?.is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    fn is_running(&self, mode: BinanceMode) -> Result<bool, StorageError> {
        Ok(self
            .read_status_file(mode)?
            .is_some_and(|status| status.pid > 0 && is_process_alive(status.pid)))
    }

    fn resolve_recorder_binary_path(&self) -> Result<PathBuf, StorageError> {
        let current_exe =
            std::env::current_exe().map_err(|error| StorageError::WriteFailedWithContext {
                message: error.to_string(),
            })?;
        let Some(dir) = current_exe.parent() else {
            return Err(StorageError::WriteFailedWithContext {
                message: "failed to resolve current executable directory".to_string(),
            });
        };
        let binary_name = if cfg!(windows) {
            "sandbox-quant-recorder.exe"
        } else {
            "sandbox-quant-recorder"
        };
        let path = dir.join(binary_name);
        if path.exists() {
            Ok(path)
        } else {
            Err(StorageError::WriteFailedWithContext {
                message: format!("recorder binary not found: {}", path.display()),
            })
        }
    }

    fn write_config(
        &self,
        mode: BinanceMode,
        config: &RecordConfigFile,
    ) -> Result<(), StorageError> {
        fs::create_dir_all(&self.base_dir).map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?;
        let payload = serde_json::to_string_pretty(config).map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?;
        atomic_write(self.config_path(mode), &payload)
    }

    pub fn write_status_file(
        &self,
        mode: BinanceMode,
        status: &RecordStatusFile,
    ) -> Result<(), StorageError> {
        fs::create_dir_all(&self.base_dir).map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?;
        let payload = serde_json::to_string_pretty(status).map_err(|error| {
            StorageError::WriteFailedWithContext {
                message: error.to_string(),
            }
        })?;
        atomic_write(self.status_path(mode), &payload)
    }

    fn read_config(&self, mode: BinanceMode) -> Result<Option<RecordConfigFile>, StorageError> {
        read_json_file(self.config_path(mode))
    }

    fn read_status_file(
        &self,
        mode: BinanceMode,
    ) -> Result<Option<RecordStatusFile>, StorageError> {
        read_json_file(self.status_path(mode))
    }

    pub fn config_path(&self, mode: BinanceMode) -> PathBuf {
        self.base_dir
            .join(format!("record-{}.config.json", format_mode(mode)))
    }

    pub fn status_path(&self, mode: BinanceMode) -> PathBuf {
        self.base_dir
            .join(format!("record-{}.status.json", format_mode(mode)))
    }

    pub fn db_path(&self, mode: BinanceMode) -> PathBuf {
        self.base_dir
            .join(format!("market-{}.duckdb", format_mode(mode)))
    }
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: PathBuf) -> Result<Option<T>, StorageError> {
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(path).map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let parsed =
        serde_json::from_str(&content).map_err(|error| StorageError::WriteFailedWithContext {
            message: error.to_string(),
        })?;
    Ok(Some(parsed))
}

fn merge_symbols(left: Vec<String>, right: Vec<String>) -> Vec<String> {
    let mut merged = std::collections::BTreeSet::new();
    for symbol in left.into_iter().chain(right.into_iter()) {
        merged.insert(symbol);
    }
    merged.into_iter().collect()
}

fn atomic_write(path: PathBuf, payload: &str) -> Result<(), StorageError> {
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("json")
    ));
    fs::write(&tmp_path, payload).map_err(|error| StorageError::WriteFailedWithContext {
        message: error.to_string(),
    })?;
    fs::rename(&tmp_path, &path).map_err(|error| StorageError::WriteFailedWithContext {
        message: error.to_string(),
    })?;
    Ok(())
}

pub fn format_mode(mode: BinanceMode) -> &'static str {
    match mode {
        BinanceMode::Real => "real",
        BinanceMode::Demo => "demo",
    }
}

fn is_process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn terminate_process(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, libc::SIGTERM) == 0 }
}
