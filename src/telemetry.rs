use anyhow::Result;
use std::io::IsTerminal;
use std::sync::OnceLock;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

pub fn init() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let ansi_stderr = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();

    let log_dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "logs".to_owned());
    let file_appender = tracing_appender::rolling::never(log_dir, "output.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);
    let timer = fmt::time::ChronoLocal::new("%H:%M:%S.%6f".to_string());

    let stderr_layer = fmt::layer()
        .with_timer(timer.clone())
        .with_target(true)
        .with_ansi(ansi_stderr);

    let file_layer = fmt::layer()
        .with_timer(timer)
        .with_target(true)
        .with_writer(non_blocking)
        .with_ansi(false)
        .json()
        .with_current_span(true)
        .with_span_list(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .try_init()?;

    tracing::info!("Logger initialized");
    Ok(())
}
