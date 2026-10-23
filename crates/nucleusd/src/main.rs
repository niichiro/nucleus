// crates/nucleusd/src/main.rs — тонкий bin: вся логика в библиотеке

use std::num::NonZeroUsize;

use clap::Parser;
use nucleus_core::{CoreError, FloodRouter, NucleusCore, TransportError};
use nucleusd::{
    ble,
    config::{Cli, Config},
    devices::DeviceManager,
    http, mdns, mqtt,
    state::StateStore,
};
use tokio::io::AsyncBufReadExt as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = Config::load(&cli)?;

    // Логи: консоль + суточные rolling-файлы в data_dir/logs
    let file_appender =
        tracing_appender::rolling::daily(config.data_dir.join("logs"), "nucleusd.log");
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(true),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_appender)
                .with_ansi(false),
        )
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("nucleusd starting: {config:?}");

    // Устройства из state.json (атомарный снапшот на каждое изменение)
    let store = StateStore::load_or_default(config.data_dir.join("state.json"));
    let manager = DeviceManager::new(store);
    for entry in manager.entries() {
        tracing::info!("device: {} ({:?})", entry.id, entry.state);
    }

    // HTTP API
    tokio::spawn(http::serve(manager.clone(), config.http_port));

    // MQTT-мост: живёт с переподключениями, демон от него не зависит
    {
        let manager = manager.clone();
        let broker = config.mqtt_broker.clone();
        tokio::spawn(async move {
            mqtt::run(manager, &broker).await;
        });
    }

    // mDNS-обнаружение
    if config.mdns {
        match mdns::register(&config.node_name, config.http_port) {
            Ok(_daemon) => {}
            Err(e) => tracing::warn!("mDNS unavailable: {e}"),
        }
    }

    // BLE-периферия + движок протокола
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;
    let (transport, ble_task) = ble::start(&adapter).await?;

    let router = FloodRouter::new(NonZeroUsize::new(1024).unwrap(), 8);
    let mut core = NucleusCore::new(manager.clone(), router, transport);
    let engine_task = tokio::spawn(async move {
        loop {
            match core.run_once().await {
                Ok(()) => {}
                // Битые кадры и неизвестные версии — drop + лог, работаем дальше
                Err(CoreError::Transport(TransportError::Closed)) => {
                    tracing::warn!("BLE transport closed, engine loop stopped");
                    break;
                }
                Err(e) => tracing::warn!("engine: {e}"),
            }
        }
    });

    println!(
        "Nucleus node up: HTTP :{}, BLE advertising as \"nu\". Press Enter to quit.",
        config.http_port
    );
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let _ = lines.next_line().await;

    engine_task.abort();
    ble_task.abort();
    Ok(())
}
