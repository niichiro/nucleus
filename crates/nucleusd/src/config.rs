// crates/nucleusd/src/config.rs

use std::{error::Error, fs, path::PathBuf};

use clap::Parser;
use serde::Deserialize;

/// Аргументы командной строки nucleusd.
#[derive(Debug, Parser)]
#[command(name = "nucleusd", about = "Демон Nucleus: узел умного дома", version)]
pub struct Cli {
    /// Каталог данных: state.json, логи
    #[arg(long, default_value = ".")]
    pub data_dir: PathBuf,

    /// Путь к TOML-конфигу
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Порт HTTP API (переопределяет конфиг)
    #[arg(long)]
    pub http_port: Option<u16>,

    /// Адрес MQTT-брокера host:port (переопределяет конфиг)
    #[arg(long)]
    pub mqtt_broker: Option<String>,

    /// Отключить mDNS-обнаружение
    #[arg(long)]
    pub no_mdns: bool,
}

/// Секция TOML-конфига (все поля опциональны).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FileConfig {
    http_port: Option<u16>,
    mqtt_broker: Option<String>,
    node_name: Option<String>,
}

/// Итоговая конфигурация: дефолты ← TOML ← аргументы CLI.
#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub http_port: u16,
    pub mqtt_broker: String,
    pub node_name: String,
    pub mdns: bool,
}

impl Config {
    pub fn load(cli: &Cli) -> Result<Config, Box<dyn Error>> {
        let file: FileConfig = match &cli.config {
            Some(path) => {
                let text = fs::read_to_string(path)?;
                toml::from_str(&text).map_err(|e| format!("config {path:?}: {e}"))?
            }
            None => FileConfig::default(),
        };

        Ok(Config {
            data_dir: cli.data_dir.clone(),
            http_port: cli.http_port.or(file.http_port).unwrap_or(3000),
            mqtt_broker: cli
                .mqtt_broker
                .clone()
                .or(file.mqtt_broker)
                .unwrap_or_else(|| "127.0.0.1:1883".into()),
            node_name: file
                .node_name
                .or_else(|| std::env::var("HOSTNAME").ok())
                .unwrap_or_else(|| "nucleus".into()),
            mdns: !cli.no_mdns,
        })
    }
}
