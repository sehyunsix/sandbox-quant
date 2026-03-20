use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::writer::{BoxMakeWriter, MakeWriter};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct TeeMakeWriter {
    file: Option<Arc<Mutex<File>>>,
}

struct TeeWriter {
    file: Option<Arc<Mutex<File>>>,
}

impl<'a> MakeWriter<'a> for TeeMakeWriter {
    type Writer = TeeWriter;

    fn make_writer(&'a self) -> Self::Writer {
        TeeWriter {
            file: self.file.clone(),
        }
    }
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut stderr = io::stderr().lock();
        stderr.write_all(buf)?;
        if let Some(file) = &self.file {
            let mut file = file
                .lock()
                .map_err(|_| io::Error::other("failed to lock log file"))?;
            file.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut stderr = io::stderr().lock();
        stderr.flush()?;
        if let Some(file) = &self.file {
            let mut file = file
                .lock()
                .map_err(|_| io::Error::other("failed to lock log file"))?;
            file.flush()?;
        }
        Ok(())
    }
}

pub fn init_logging(service: &str, mode: Option<&str>) -> io::Result<()> {
    let writer = build_writer(service, mode)?;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sandbox_quant=info,reqwest=warn,hyper=warn"));
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_ansi(false)
        .finish();

    if tracing::subscriber::set_global_default(subscriber).is_ok() {
        install_panic_hook(service, mode);
    }
    Ok(())
}

fn build_writer(service: &str, mode: Option<&str>) -> io::Result<BoxMakeWriter> {
    let log_dir = std::env::var("SANDBOX_QUANT_LOG_DIR").unwrap_or_else(|_| "var/log".to_string());
    let file = open_log_file(Path::new(&log_dir), service, mode)?;
    Ok(BoxMakeWriter::new(TeeMakeWriter { file }))
}

fn open_log_file(
    dir: &Path,
    service: &str,
    mode: Option<&str>,
) -> io::Result<Option<Arc<Mutex<File>>>> {
    create_dir_all(dir)?;
    let mut filename = sanitize_component(service);
    if let Some(mode) = mode.filter(|mode| !mode.trim().is_empty()) {
        filename.push('-');
        filename.push_str(&sanitize_component(mode));
    }
    filename.push_str(".jsonl");
    let path: PathBuf = dir.join(filename);
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    Ok(Some(Arc::new(Mutex::new(file))))
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn install_panic_hook(service: &str, mode: Option<&str>) {
    let service = service.to_string();
    let mode = mode.map(str::to_string);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location = panic_info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown".to_string());
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| value.to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic payload".to_string());
        tracing::error!(
            service = service,
            mode = mode.as_deref().unwrap_or("unknown"),
            location = location,
            panic = payload,
            "process panicked"
        );
        previous(panic_info);
    }));
}
