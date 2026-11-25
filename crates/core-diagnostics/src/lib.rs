//! Persistent rolling debug logging, panic/crash report capture, and redacted
//! diagnostic bundle export shared by `trueshow-engine` and `apps/desktop`.
//!
//! Privacy-first & offline-first: nothing is ever transmitted over the network automatically.
//! Operators inspect logs/crashes locally or export a redacted Markdown bundle to attach to
//! an issue report.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::backtrace::Backtrace;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;
use uuid::Uuid;

/// Default maximum bytes per active log file before rotating (5 MiB).
pub const DEFAULT_MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
/// Default rotated log generations kept per component (`engine.log.1` .. `.5`).
pub const DEFAULT_MAX_LOG_FILES: usize = 5;
/// Default maximum persisted JSON crash reports kept in `crashes/`.
pub const DEFAULT_MAX_CRASH_REPORTS: usize = 25;

#[derive(Debug, Error)]
pub enum DiagnosticsError {
    #[error("I/O error in diagnostics subsystem: {0}")]
    Io(#[from] io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid log level '{0}' (expected one of: error, warn, info, debug, trace)")]
    InvalidLogLevel(String),
    #[error("invalid log source '{0}' (expected one of: engine, desktop, all)")]
    InvalidLogSource(String),
    #[error("failed to reload active log filter: {0}")]
    FilterReload(String),
}

/// Supported runtime logging verbosity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    pub fn parse(input: &str) -> Result<Self, DiagnosticsError> {
        match input.trim().to_ascii_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "warn" | "warning" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            other => Err(DiagnosticsError::InvalidLogLevel(other.to_string())),
        }
    }

    /// Builds a `tracing_subscriber::EnvFilter` directive string that keeps noisy
    /// third-party shader/font crates quiet at `debug`, while opening them up at `trace`.
    pub fn filter_directive(self) -> String {
        match self {
            Self::Trace => "trace,naga=info".to_string(),
            Self::Debug => {
                "debug,wgpu=warn,wgpu_hal=warn,wgpu_core=warn,naga=warn,cosmic_text=warn"
                    .to_string()
            }
            Self::Info => "info,wgpu=warn,wgpu_hal=warn,naga=warn,cosmic_text=warn".to_string(),
            Self::Warn => "warn".to_string(),
            Self::Error => "error".to_string(),
        }
    }
}

/// Filesystem layout for TrueShow logs and crash reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsPaths {
    pub root_dir: PathBuf,
    pub log_dir: PathBuf,
    pub crash_dir: PathBuf,
}

impl DiagnosticsPaths {
    /// Resolves OS-standard local diagnostics directories (overridable via `TRUESHOW_LOG_DIR`).
    pub fn default_system() -> Self {
        let root = if let Ok(custom) = std::env::var("TRUESHOW_LOG_DIR")
            && !custom.trim().is_empty()
        {
            PathBuf::from(custom)
        } else {
            default_os_root_dir()
        };
        Self::from_root(root)
    }

    pub fn from_root(root_dir: impl Into<PathBuf>) -> Self {
        let root_dir = root_dir.into();
        let log_dir = root_dir.join("logs");
        let crash_dir = root_dir.join("crashes");
        Self {
            root_dir,
            log_dir,
            crash_dir,
        }
    }

    pub fn ensure_dirs(&self) -> Result<(), DiagnosticsError> {
        fs::create_dir_all(&self.log_dir)?;
        fs::create_dir_all(&self.crash_dir)?;
        Ok(())
    }

    pub fn log_file_for(&self, component: &str) -> PathBuf {
        let safe = sanitize_filename_token(component);
        self.log_dir.join(format!("{safe}.log"))
    }
}

fn default_os_root_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA")
            && !local_app_data.trim().is_empty()
        {
            return PathBuf::from(local_app_data).join("TrueShow");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME")
            && !home.trim().is_empty()
        {
            return PathBuf::from(home)
                .join("Library")
                .join("Logs")
                .join("TrueShow");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(state) = std::env::var("XDG_STATE_HOME")
            && !state.trim().is_empty()
        {
            return PathBuf::from(state).join("trueshow");
        }
        if let Ok(home) = std::env::var("HOME")
            && !home.trim().is_empty()
        {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("trueshow");
        }
    }
    std::env::temp_dir().join("trueshow-diagnostics")
}

fn sanitize_filename_token(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "app".to_string()
    } else {
        trimmed.to_string()
    }
}

struct RollingFileInner {
    path: PathBuf,
    file: File,
    bytes_written: u64,
    max_bytes: u64,
    max_files: usize,
}

impl RollingFileInner {
    fn open(path: PathBuf, max_bytes: u64, max_files: usize) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let bytes_written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            file,
            bytes_written,
            max_bytes: max_bytes.max(256),
            max_files: max_files.max(1),
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        let _ = self.file.flush();
        for idx in (1..self.max_files).rev() {
            let src = rotated_path(&self.path, idx);
            let dst = rotated_path(&self.path, idx + 1);
            if src.exists() {
                let _ = fs::remove_file(&dst);
                let _ = fs::rename(&src, &dst);
            }
        }
        let first_backup = rotated_path(&self.path, 1);
        let _ = fs::remove_file(&first_backup);
        // Replace active handle with a fresh file after moving current log aside.
        // On Windows, close the open handle first by swapping with a temporary handle or reopening.
        let temp_swap = self.path.with_extension("log.new");
        let fresh = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp_swap)?;
        let old = std::mem::replace(&mut self.file, fresh);
        drop(old);

        let _ = fs::rename(&self.path, &first_backup);
        let _ = fs::rename(&temp_swap, &self.path);
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.bytes_written = 0;
        Ok(())
    }
}

fn rotated_path(base: &Path, generation: usize) -> PathBuf {
    let name = base
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app.log".to_string());
    base.with_file_name(format!("{name}.{generation}"))
}

/// Thread-safe size-rotating log file writer compatible with `tracing_subscriber`.
#[derive(Clone)]
pub struct RollingFileWriter {
    inner: Arc<Mutex<RollingFileInner>>,
}

impl RollingFileWriter {
    pub fn new(
        path: impl Into<PathBuf>,
        max_bytes: u64,
        max_files: usize,
    ) -> Result<Self, DiagnosticsError> {
        let inner = RollingFileInner::open(path.into(), max_bytes, max_files)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    pub fn write_bytes(&self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("rolling log mutex poisoned"))?;
        if guard.bytes_written.saturating_add(buf.len() as u64) > guard.max_bytes
            && guard.bytes_written > 0
        {
            guard.rotate()?;
        }
        let n = guard.file.write(buf)?;
        guard.bytes_written = guard.bytes_written.saturating_add(n as u64);
        Ok(n)
    }

    pub fn flush_all(&self) -> io::Result<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("rolling log mutex poisoned"))?;
        guard.file.flush()
    }
}

pub struct RollingFileGuard {
    writer: RollingFileWriter,
}

impl Write for RollingFileGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write_bytes(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush_all()
    }
}

impl<'a> MakeWriter<'a> for RollingFileWriter {
    type Writer = RollingFileGuard;

    fn make_writer(&'a self) -> Self::Writer {
        RollingFileGuard {
            writer: self.clone(),
        }
    }
}

/// Runtime controller for dynamically updating the process log verbosity level.
#[derive(Clone)]
pub struct LoggingController {
    level: Arc<RwLock<LogLevel>>,
    reload_fn: Arc<dyn Fn(EnvFilter) -> Result<(), String> + Send + Sync>,
    log_file: PathBuf,
}

impl LoggingController {
    pub fn current_level(&self) -> LogLevel {
        self.level.read().map(|g| *g).unwrap_or(LogLevel::Info)
    }

    pub fn log_file(&self) -> &Path {
        &self.log_file
    }

    pub fn set_level(&self, level: LogLevel) -> Result<(), DiagnosticsError> {
        let filter = EnvFilter::try_new(level.filter_directive())
            .map_err(|e| DiagnosticsError::FilterReload(e.to_string()))?;
        (self.reload_fn)(filter).map_err(DiagnosticsError::FilterReload)?;
        if let Ok(mut guard) = self.level.write() {
            *guard = level;
        }
        tracing::info!(new_level = level.as_str(), "updated active log verbosity");
        Ok(())
    }
}

/// Initializes structured tracing to BOTH terminal (`stderr`) and a rolling file under
/// `paths.log_dir/<component>.log`, returning a live [`LoggingController`].
pub fn init_logging(
    component: &str,
    paths: &DiagnosticsPaths,
    default_level: LogLevel,
) -> Result<LoggingController, DiagnosticsError> {
    paths.ensure_dirs()?;
    let log_file = paths.log_file_for(component);
    let file_writer =
        RollingFileWriter::new(&log_file, DEFAULT_MAX_LOG_BYTES, DEFAULT_MAX_LOG_FILES)?;

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_level.filter_directive()))
        .map_err(|e| DiagnosticsError::FilterReload(e.to_string()))?;
    let (filter_layer, reload_handle) = reload::Layer::new(env_filter);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(true)
        .with_writer(file_writer);

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(io::stderr)
        .with_target(true);

    let _ = tracing_subscriber::registry()
        .with(filter_layer)
        .with(stderr_layer)
        .with(file_layer)
        .try_init();

    let reload_fn = Arc::new(move |new_filter: EnvFilter| {
        reload_handle.reload(new_filter).map_err(|e| e.to_string())
    });

    Ok(LoggingController {
        level: Arc::new(RwLock::new(default_level)),
        reload_fn,
        log_file,
    })
}

/// Structured offline crash / fatal runtime error report persisted under `crashes/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashReport {
    pub id: String,
    pub timestamp_utc: DateTime<Utc>,
    pub component: String,
    pub kind: String,
    pub app_version: String,
    pub pid: u32,
    pub os: String,
    pub arch: String,
    pub thread: String,
    pub message: String,
    pub location: Option<String>,
    pub backtrace: String,
    pub recent_logs: Vec<String>,
}

/// Installs a panic hook that persists a structured JSON [`CrashReport`] (with backtrace and
/// recent log tail) before forwarding to any previously installed panic hook.
pub fn install_crash_hook(
    component: impl Into<String>,
    app_version: impl Into<String>,
    paths: DiagnosticsPaths,
) {
    let component = component.into();
    let app_version = app_version.into();
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = extract_panic_message(info);
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let backtrace = Backtrace::force_capture().to_string();
        let _ = record_runtime_error(
            &paths,
            &component,
            "panic",
            &app_version,
            &message,
            location.as_deref(),
            Some(&backtrace),
        );
        prev_hook(info);
    }));
}

fn extract_panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Records a structured crash or fatal runtime error report (GPU device error, abnormal sidecar
/// exit, uncaught frontend JS exception, or Rust panic) to `paths.crash_dir`.
pub fn record_runtime_error(
    paths: &DiagnosticsPaths,
    component: &str,
    kind: &str,
    app_version: &str,
    message: &str,
    location: Option<&str>,
    backtrace: Option<&str>,
) -> Result<CrashReport, DiagnosticsError> {
    paths.ensure_dirs()?;

    let log_source = if component.contains("engine") {
        "engine"
    } else {
        "desktop"
    };
    let recent_logs = read_recent_logs(paths, log_source, 40).unwrap_or_default();
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let thread = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_string();

    let report = CrashReport {
        id: id.clone(),
        timestamp_utc: now,
        component: component.to_string(),
        kind: kind.to_string(),
        app_version: app_version.to_string(),
        pid: std::process::id(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        thread,
        message: message.to_string(),
        location: location.map(str::to_string),
        backtrace: backtrace
            .map(str::to_string)
            .unwrap_or_else(|| Backtrace::force_capture().to_string()),
        recent_logs,
    };

    let filename = format!(
        "crash-{}-{}-{}-{}.json",
        now.format("%Y%m%dT%H%M%SZ"),
        sanitize_filename_token(component),
        std::process::id(),
        &id[..8]
    );
    let path = paths.crash_dir.join(filename);
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&path, json)?;
    let _ = prune_crash_reports(paths, DEFAULT_MAX_CRASH_REPORTS);
    Ok(report)
}

/// Prunes older crash reports so at most `max_keep` JSON files remain in `paths.crash_dir`.
pub fn prune_crash_reports(
    paths: &DiagnosticsPaths,
    max_keep: usize,
) -> Result<usize, DiagnosticsError> {
    if !paths.crash_dir.exists() {
        return Ok(0);
    }
    let mut entries: Vec<(String, PathBuf)> = fs::read_dir(&paths.crash_dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .map(|p| {
            let key = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (key, p)
        })
        .collect();

    entries.sort_by(|a, b| b.0.cmp(&a.0));
    let mut removed = 0usize;
    for (_, path) in entries.into_iter().skip(max_keep) {
        if fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Lists all valid persisted crash reports sorted newest-first.
pub fn list_crash_reports(paths: &DiagnosticsPaths) -> Result<Vec<CrashReport>, DiagnosticsError> {
    if !paths.crash_dir.exists() {
        return Ok(Vec::new());
    }
    let mut reports = Vec::new();
    for entry in fs::read_dir(&paths.crash_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path)
            && let Ok(report) = serde_json::from_str::<CrashReport>(&raw)
        {
            reports.push(report);
        }
    }
    reports.sort_by_key(|a| std::cmp::Reverse(a.timestamp_utc));
    Ok(reports)
}

/// Deletes all persisted JSON crash reports from `paths.crash_dir`.
pub fn clear_crash_reports(paths: &DiagnosticsPaths) -> Result<usize, DiagnosticsError> {
    if !paths.crash_dir.exists() {
        return Ok(0);
    }
    let mut removed = 0usize;
    for entry in fs::read_dir(&paths.crash_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json")
            && fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Reads the trailing `max_lines` log entries from `"engine"`, `"desktop"`, or `"all"`.
pub fn read_recent_logs(
    paths: &DiagnosticsPaths,
    source: &str,
    max_lines: usize,
) -> Result<Vec<String>, DiagnosticsError> {
    let limit = max_lines.clamp(1, 2000);
    match source.trim().to_ascii_lowercase().as_str() {
        "engine" => tail_file_lines(&paths.log_file_for("engine"), limit),
        "desktop" => tail_file_lines(&paths.log_file_for("desktop"), limit),
        "all" => {
            let mut merged = Vec::new();
            for line in tail_file_lines(&paths.log_file_for("engine"), limit)? {
                merged.push(format!("[engine] {line}"));
            }
            for line in tail_file_lines(&paths.log_file_for("desktop"), limit)? {
                merged.push(format!("[desktop] {line}"));
            }
            if merged.len() > limit {
                let start = merged.len() - limit;
                merged.drain(..start);
            }
            Ok(merged)
        }
        other => Err(DiagnosticsError::InvalidLogSource(other.to_string())),
    }
}

fn tail_file_lines(path: &Path, max_lines: usize) -> Result<Vec<String>, DiagnosticsError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let lines: Vec<String> = content.lines().map(str::to_string).collect();
    if lines.len() <= max_lines {
        Ok(lines)
    } else {
        Ok(lines[lines.len() - max_lines..].to_vec())
    }
}

/// Redacts OS user home directories and common API token headers from diagnostic text
/// before sharing or attaching to a bug report.
pub fn redact_sensitive_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for line in input.lines() {
        let mut sanitized = redact_user_paths(line);
        sanitized = redact_secret_tokens(&sanitized);
        out.push_str(&sanitized);
        out.push('\n');
    }
    if !input.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

fn redact_user_paths(line: &str) -> String {
    let mut current = line.to_string();
    for (prefix, sep) in [("C:\\Users\\", '\\'), ("/Users/", '/'), ("/home/", '/')] {
        let mut search_from = 0usize;
        while let Some(rel_idx) = current[search_from..].find(prefix) {
            let start = search_from + rel_idx + prefix.len();
            let rest = &current[start..];
            let user_len = rest
                .find(|c: char| c == sep || c.is_whitespace() || c == '"' || c == '\'' || c == ')')
                .unwrap_or(rest.len());
            if user_len > 0 && &rest[..user_len] != "<redacted>" {
                current.replace_range(start..start + user_len, "<redacted>");
                search_from = start + "<redacted>".len();
            } else {
                search_from = start + user_len;
            }
        }
    }
    current
}

fn redact_secret_tokens(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    for marker in ["bearer ", "api-key:", "authorization:", "token="] {
        if let Some(pos) = lower.find(marker) {
            let val_start = pos + marker.len();
            let prefix = &line[..val_start];
            return format!("{prefix} <redacted>");
        }
    }
    line.to_string()
}

/// Result of exporting a self-contained diagnostic bundle for issue reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedBundle {
    pub file_path: String,
    pub crash_count: usize,
    pub markdown: String,
}

/// Exports a redacted Markdown diagnostic bundle containing system info, GPU summary,
/// latest crash reports, and recent `engine.log` + `desktop.log` tails.
pub fn export_diagnostic_bundle(
    paths: &DiagnosticsPaths,
    app_version: &str,
    active_log_level: LogLevel,
    gpu_summary: Option<&str>,
) -> Result<ExportedBundle, DiagnosticsError> {
    paths.ensure_dirs()?;
    let now = Utc::now();
    let crashes = list_crash_reports(paths)?;
    let engine_logs = read_recent_logs(paths, "engine", 150)?;
    let desktop_logs = read_recent_logs(paths, "desktop", 150)?;

    let mut md = String::new();
    md.push_str("# TrueShow Diagnostic Bundle\n\n");
    md.push_str(&format!("- **Generated (UTC):** `{}`\n", now.to_rfc3339()));
    md.push_str(&format!("- **Version:** `{app_version}`\n"));
    md.push_str(&format!(
        "- **Platform:** `{}` / `{}`\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    md.push_str(&format!(
        "- **Active Log Level:** `{}`\n",
        active_log_level.as_str()
    ));
    if let Some(gpu) = gpu_summary {
        md.push_str(&format!("- **GPU / Engine:** `{gpu}`\n"));
    }
    md.push_str(&format!("- **Recorded Crashes:** `{}`\n\n", crashes.len()));

    md.push_str("## Recent Crash Reports\n\n");
    if crashes.is_empty() {
        md.push_str("_No crash reports recorded._\n\n");
    } else {
        for report in crashes.iter().take(5) {
            md.push_str(&format!(
                "### `{}` — {} (`{}`)\n",
                report.timestamp_utc.to_rfc3339(),
                report.component,
                report.kind
            ));
            md.push_str(&format!("- **Message:** `{}`\n", report.message));
            if let Some(loc) = &report.location {
                md.push_str(&format!("- **Location:** `{loc}`\n"));
            }
            md.push_str(&format!(
                "- **Thread / PID:** `{}` / `{}`\n\n```text\n{}\n```\n\n",
                report.thread, report.pid, report.backtrace
            ));
        }
    }

    md.push_str("## Recent Engine Log (`engine.log`)\n\n```text\n");
    for line in &engine_logs {
        md.push_str(line);
        md.push('\n');
    }
    md.push_str("```\n\n## Recent Desktop Log (`desktop.log`)\n\n```text\n");
    for line in &desktop_logs {
        md.push_str(line);
        md.push('\n');
    }
    md.push_str("```\n");

    let redacted = redact_sensitive_text(&md);
    let out_file = paths.log_dir.join(format!(
        "trueshow-diagnostics-{}.md",
        now.format("%Y%m%dT%H%M%SZ")
    ));
    fs::write(&out_file, &redacted)?;

    Ok(ExportedBundle {
        file_path: out_file.to_string_lossy().into_owned(),
        crash_count: crashes.len(),
        markdown: redacted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(tag: &str) -> DiagnosticsPaths {
        let dir = std::env::temp_dir().join(format!("trueshow-diag-test-{tag}-{}", Uuid::new_v4()));
        DiagnosticsPaths::from_root(dir)
    }

    #[test]
    fn parses_log_levels_and_rejects_invalid_values() {
        assert_eq!(LogLevel::parse("info").expect("info"), LogLevel::Info);
        assert_eq!(LogLevel::parse("DEBUG").expect("debug"), LogLevel::Debug);
        assert_eq!(LogLevel::parse("trace").expect("trace"), LogLevel::Trace);
        assert_eq!(LogLevel::parse("warning").expect("warn"), LogLevel::Warn);
        assert_eq!(LogLevel::parse("error").expect("error"), LogLevel::Error);

        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Trace.as_str(), "trace");
        assert_eq!(LogLevel::Warn.as_str(), "warn");
        assert_eq!(LogLevel::Error.as_str(), "error");

        assert!(LogLevel::Debug.filter_directive().contains("wgpu=warn"));
        assert!(LogLevel::Trace.filter_directive().contains("trace"));
        assert!(LogLevel::Info.filter_directive().contains("info"));
        assert_eq!(LogLevel::Warn.filter_directive(), "warn");
        assert_eq!(LogLevel::Error.filter_directive(), "error");

        assert!(matches!(
            LogLevel::parse("verbose"),
            Err(DiagnosticsError::InvalidLogLevel(_))
        ));
    }

    #[test]
    fn rolling_log_writer_rotates_files_and_tails_cleanly() {
        let paths = temp_paths("rolling");
        paths.ensure_dirs().expect("ensure dirs");

        let engine_log = paths.log_file_for("engine");
        let writer = RollingFileWriter::new(&engine_log, 300, 3).expect("create writer");
        for i in 0..15 {
            let line = format!(
                "2026-11-24T10:00:{i:02}Z INFO trueshow_engine line={i} padding_data_0123456789\n"
            );
            let mut guard = writer.make_writer();
            guard.write_all(line.as_bytes()).expect("write line");
            guard.flush().expect("flush");
        }

        // Rotated generations should exist.
        assert!(rotated_path(&engine_log, 1).exists());

        let desktop_log = paths.log_file_for("desktop");
        fs::write(&desktop_log, "desktop line 1\ndesktop line 2\n").expect("write desktop log");

        let engine_tail = read_recent_logs(&paths, "engine", 5).expect("tail engine");
        assert!(!engine_tail.is_empty());
        assert!(engine_tail.len() <= 5);

        let desktop_tail = read_recent_logs(&paths, "desktop", 10).expect("tail desktop");
        assert_eq!(desktop_tail.len(), 2);

        let merged = read_recent_logs(&paths, "all", 4).expect("tail all");
        assert!(merged.len() <= 4);
        assert!(merged.iter().any(|l| l.starts_with("[desktop]")));

        assert!(matches!(
            read_recent_logs(&paths, "invalid-source", 10),
            Err(DiagnosticsError::InvalidLogSource(_))
        ));

        let _ = fs::remove_dir_all(&paths.root_dir);
    }

    #[test]
    fn records_lists_prunes_clears_crashes_and_exports_redacted_bundle() {
        let paths = temp_paths("crashes");
        // Empty state before dirs exist
        assert!(list_crash_reports(&paths).expect("empty list").is_empty());
        assert_eq!(clear_crash_reports(&paths).expect("empty clear"), 0);
        assert_eq!(prune_crash_reports(&paths, 5).expect("empty prune"), 0);
        assert!(
            read_recent_logs(&paths, "engine", 10)
                .expect("missing file")
                .is_empty()
        );

        paths.ensure_dirs().expect("ensure dirs");
        fs::write(
            paths.log_file_for("engine"),
            "INFO started from C:\\Users\\PastorBob\\TrueShow\nDEBUG Authorization: secret-xyz-123\n",
        )
        .expect("write log");
        fs::write(
            paths.log_file_for("desktop"),
            "INFO loaded /Users/alice/songs and /home/bob/media Bearer supersecrettoken\n",
        )
        .expect("write desktop log");

        // Write a corrupt JSON file to verify list_crash_reports skips it safely.
        fs::write(paths.crash_dir.join("corrupt.json"), "{ not valid json").expect("corrupt");

        let r1 = record_runtime_error(
            &paths,
            "trueshow-engine",
            "wgpu-device-error",
            "0.1.0-m3",
            "Vulkan swapchain surface lost",
            Some("crates/render-wgpu/src/lib.rs:510"),
            Some("frame 0: render_output"),
        )
        .expect("record r1");
        assert_eq!(r1.component, "trueshow-engine");
        assert!(!r1.recent_logs.is_empty());

        let r2 = record_runtime_error(
            &paths,
            "trueshow-ui",
            "ui-unhandled-error",
            "0.1.0-m3",
            "TypeError: cannot read properties of null",
            None,
            None,
        )
        .expect("record r2");
        assert_eq!(r2.kind, "ui-unhandled-error");

        let listed = list_crash_reports(&paths).expect("list crashes");
        assert_eq!(listed.len(), 2);

        let bundle = export_diagnostic_bundle(
            &paths,
            "0.1.0-m3",
            LogLevel::Debug,
            Some("NVIDIA GeForce RTX 5080 (Vulkan)"),
        )
        .expect("export bundle");
        assert_eq!(bundle.crash_count, 2);
        assert!(Path::new(&bundle.file_path).exists());
        assert!(bundle.markdown.contains("C:\\Users\\<redacted>\\TrueShow"));
        assert!(bundle.markdown.contains("/Users/<redacted>/songs"));
        assert!(bundle.markdown.contains("/home/<redacted>/media"));
        assert!(!bundle.markdown.contains("PastorBob"));
        assert!(!bundle.markdown.contains("secret-xyz-123"));
        assert!(!bundle.markdown.contains("supersecrettoken"));

        // Prune down to 1 crash report (corrupt.json + 2 valid = 3 json files -> keeps 1).
        let pruned = prune_crash_reports(&paths, 1).expect("prune");
        assert_eq!(pruned, 2);

        let cleared = clear_crash_reports(&paths).expect("clear");
        assert_eq!(cleared, 1);
        assert!(list_crash_reports(&paths).expect("after clear").is_empty());

        let _ = fs::remove_dir_all(&paths.root_dir);
    }

    #[test]
    fn initializes_logging_controller_and_reloads_level() {
        let paths = temp_paths("logging-ctrl");
        let ctrl = init_logging("test-component", &paths, LogLevel::Info).expect("init logging");
        assert_eq!(ctrl.current_level(), LogLevel::Info);
        assert!(ctrl.log_file().ends_with("test-component.log"));

        ctrl.set_level(LogLevel::Debug).expect("reload to debug");
        assert_eq!(ctrl.current_level(), LogLevel::Debug);

        // Verify default system paths resolve cleanly and sanitize weird tokens.
        let sys = DiagnosticsPaths::default_system();
        assert!(sys.log_dir.ends_with("logs"));
        assert_eq!(sanitize_filename_token("!!!"), "app");

        let _ = fs::remove_dir_all(&paths.root_dir);
    }
}
