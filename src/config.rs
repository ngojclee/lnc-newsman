use anyhow::{Context, Result};
use std::{env, net::IpAddr, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub regions_dir: PathBuf,
    pub bind_address: IpAddr,
    pub port: u16,
    pub management_key: Option<String>,
    pub api_keys: Vec<String>,
    pub allowed_hosts: Vec<String>,
    pub secure_cookie: bool,
    pub version: &'static str,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let data_dir = PathBuf::from(env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()));
        let regions_dir = env::var("REGIONS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join("regions"));
        let bind_address = env::var("BIND_ADDRESS")
            .unwrap_or_else(|_| "0.0.0.0".into())
            .parse()
            .context("BIND_ADDRESS must be an IP address")?;
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8000".into())
            .parse()
            .context("PORT must be a valid u16")?;
        let management_key = first_env(&[
            "MANAGEMENT_KEY",
            "MANAGEMENT_TOKEN",
            "LNC_ADMIN_TOKEN",
            "LNC_ADMIN_BOOTSTRAP_KEY",
        ]);
        let api_keys = env::var("LNC_API_KEYS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        let allowed_hosts = env::var("ALLOWED_HOSTS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        let secure_cookie = match env::var("COOKIE_SECURE")
            .unwrap_or_else(|_| "true".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => anyhow::bail!("COOKIE_SECURE must be true or false"),
        };
        Ok(Self {
            data_dir,
            regions_dir,
            bind_address,
            port,
            management_key,
            api_keys,
            allowed_hosts,
            secure_cookie,
            version: env!("CARGO_PKG_VERSION"),
        })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("newsman.db")
    }
}

fn first_env(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| env::var(name).ok().filter(|v| !v.trim().is_empty()))
}
