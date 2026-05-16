use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{debug, info, warn};

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub rpc: RpcSettings,
    pub server: ServerSettings,
    pub workers: WorkerSettings,
    pub logging: LoggingSettings,
}

impl Settings {
    pub fn load() -> Result<Self, ConfigError> {
        match dotenvy::dotenv() {
            Ok(path) => debug!(path = %path.display(), "Loaded .env file"),
            Err(err) => debug!(%err, ".env file was not loaded"),
        }

        info!("Loading application settings");

        let config = Config::builder()
            .add_source(File::with_name("config/default"))
            .add_source(File::with_name("config/local.toml").required(false))
            .add_source(
                Environment::with_prefix("APP")
                    .prefix_separator("__")
                    .separator("__")
                    .try_parsing(true)
                    .list_separator(",")
                    .with_list_parse_key("server.cors_allowed_origins"),
            )
            .build()
            .map_err(|err| {
                warn!(%err, "Failed to build settings");
                err
            })?;

        let settings: Self = config.try_deserialize().map_err(|err| {
            warn!(%err, "Failed to deserialize settings");
            err
        })?;

        settings.log_loaded_settings();

        Ok(settings)
    }

    pub fn rpc_endpoint(&self) -> String {
        format!("{}{}", self.rpc.url, self.rpc.api_key)
    }

    fn log_loaded_settings(&self) {
        info!(
            server_bind = %self.server.bind,
            worker_count = self.workers.count,
            rpc_rps = self.rpc.rps,
            rpc_max_concurrent = self.rpc.max_concurrent,
            rpc_max_rate_limit_retries = self.rpc.max_rate_limit_retries,
            database_max_connections = self.database.max_connections,
            cors_allowed_origins = self.server.cors_allowed_origins.len(),
            logging_level = %self.logging.level,
            logging_dir = %self.logging.dir.display(),
            "Settings loaded"
        );

        debug!(
            database_url_configured = !self.database.url.is_empty(),
            rpc_url_configured = !self.rpc.url.is_empty(),
            rpc_api_key_configured = !self.rpc.api_key.is_empty(),
            "Sensitive settings presence checked"
        );

        if self.database.url.is_empty() {
            warn!("Database URL is empty");
        }

        if self.rpc.api_key.is_empty() {
            warn!("RPC API key is empty");
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RpcSettings {
    pub api_key: String,
    pub url: String,
    pub rps: u32,
    pub max_concurrent: usize,
    pub max_rate_limit_retries: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerSettings {
    pub bind: SocketAddr,
    pub cors_allowed_origins: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WorkerSettings {
    pub count: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingSettings {
    pub level: String,
    pub dir: PathBuf,
}
