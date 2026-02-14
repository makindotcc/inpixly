use anyhow::{Context, anyhow};
use serde::Deserialize;
use std::{net::SocketAddr, path::Path, rc::Rc};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: SocketAddr,
}

fn default_bind() -> SocketAddr {
    "0.0.0.0:3000".parse().unwrap()
}

impl Config {
    pub async fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        tvix_serde::from_str_with_config(&content, |eval| {
            eval.enable_import().io_handle(Rc::new(tvix_eval::StdIO))
        })
        .map_err(|e| anyhow!("failed to parse config: {e}"))
    }
}
