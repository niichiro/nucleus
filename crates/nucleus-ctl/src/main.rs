//! nucleus-ctl — «телефон» для разработки: BLE-централ и утилита управления
//! узлами Nucleus. Работает на Windows/Linux/macOS (btleplug).

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use btleplug::api::{Central, Characteristic, Manager as _, Peripheral, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral as BlePeripheral};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use nucleus_core::{codec, Action, Command, Message, Query};
use uuid::Uuid;

/// UUID сервиса Nucleus (реклама) — как в nucleusd::ble.
const NUCLEUS_UUID_STR: &str = "a6c6a000-f18e-4bfd-a51a-c609e45263d1";
/// UUID характеристики (write + notify).
const CHAR_UUID_STR: &str = "d6c6a000-f18e-4bfd-a51a-c609e45263d1";

#[derive(Parser)]
#[command(
    name = "nucleus-ctl",
    about = "Утилита управления узлами Nucleus (BLE-централ)",
    version
)]
struct Cli {
    /// BLE-адрес узла (иначе — первый найденный по рекламе «nu»)
    #[arg(long, global = true)]
    peer: Option<String>,

    /// Таймаут поиска узла, секунды
    #[arg(long, default_value = "5", global = true)]
    scan_timeout: u64,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Найти узлы: BLE-скан + mDNS
    Discover,
    /// Включить устройство
    On { device: String },
    /// Выключить устройство
    Off { device: String },
    /// Установить уровень (яркость 0-255)
    Level { device: String, level: u8 },
    /// Запросить состояние устройств узла
    Query {
        /// Устройства (пусто — все устройства узла)
        devices: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let Cli {
        peer,
        scan_timeout,
        command,
    } = Cli::parse();
    match command {
        Cmd::Discover => discover().await,
        Cmd::On { device } => send_command(&peer, scan_timeout, device, Action::OnOff(true)).await,
        Cmd::Off { device } => {
            send_command(&peer, scan_timeout, device, Action::OnOff(false)).await
        }
        Cmd::Level { device, level } => {
            send_command(&peer, scan_timeout, device, Action::SetLevel(level)).await
        }
        Cmd::Query { devices } => query(&peer, scan_timeout, devices).await,
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as u64
}

fn new_command_id() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos() as u32
}

async fn ble_adapter() -> Result<Adapter> {
    let manager = Manager::new()
        .await
        .context("не удалось открыть BLE-менеджер")?;
    manager
        .adapters()
        .await
        .context("список адаптеров")?
        .into_iter()
        .next()
        .context("BLE-адаптер не найден")
}

async fn is_nucleus_node(p: &BlePeripheral) -> bool {
    match p.properties().await {
        Ok(Some(props)) => {
            props.local_name.as_deref() == Some("nu")
                || props
                    .services
                    .contains(&Uuid::parse_str(NUCLEUS_UUID_STR).unwrap())
        }
        _ => false,
    }
}

/// Ищет узел Nucleus: по адресу (--peer) или по рекламе «nu»/сервису Nucleus.
async fn find_node(
    central: &Adapter,
    peer: Option<&str>,
    timeout: Duration,
) -> Result<BlePeripheral> {
    central
        .start_scan(ScanFilter::default())
        .await
        .context("запуск сканирования")?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        for p in central
            .peripherals()
            .await
            .context("перечисление устройств")?
        {
            match peer {
                Some(addr) if p.address().to_string() != addr => continue,
                Some(_) => return Ok(p),
                None => {
                    if is_nucleus_node(&p).await {
                        return Ok(p);
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    bail!(
        "узел Nucleus не найден за {} с (ищем рекламу «nu»{})",
        timeout.as_secs(),
        peer.map(|a| format!(" по адресу {a}")).unwrap_or_default()
    )
}

async fn connect_and_find_char(p: &BlePeripheral) -> Result<Characteristic> {
    p.connect().await.context("BLE-соединение")?;
    p.discover_services().await.context("обход сервисов")?;
    p.characteristics()
        .into_iter()
        .find(|c| c.uuid == Uuid::parse_str(CHAR_UUID_STR).unwrap())
        .context("характеристика Nucleus не найдена на узле")
}

/// Отправляет сообщение и ждёт ответа, для которого `accept` вернул true.
async fn exchange(
    p: &BlePeripheral,
    ch: &Characteristic,
    msg: &Message,
    timeout: Duration,
    accept: impl Fn(&Message) -> bool,
) -> Result<Message> {
    p.subscribe(ch).await.context("подписка на notify")?;
    let frame = codec::encode(msg)?;
    p.write(ch, &frame, WriteType::WithoutResponse)
        .await
        .context("запись команды")?;

    let mut notifications = p.notifications().await.context("поток уведомлений")?;
    let reply = tokio::time::timeout(timeout, async {
        loop {
            let Some(n) = notifications.next().await else {
                bail!("поток уведомлений закрыт");
            };
            if n.uuid != ch.uuid {
                continue;
            }
            match codec::decode::<Message>(&n.value) {
                Ok(m) if accept(&m) => return Ok(m),
                Ok(_) => {} // чужой ответ (напр., устаревший Ack) — ждём свой
                Err(e) => eprintln!("битый кадр от узла: {e}"),
            }
        }
    })
    .await
    .context("таймаут ожидания ответа узла")??;

    p.unsubscribe(ch).await.ok();
    Ok(reply)
}

async fn send_command(
    peer: &Option<String>,
    scan_timeout: u64,
    device: String,
    action: Action,
) -> Result<()> {
    let adapter = ble_adapter().await?;
    let node = find_node(&adapter, peer.as_deref(), Duration::from_secs(scan_timeout)).await?;
    println!("узел: {}", node.address());
    let ch = connect_and_find_char(&node).await?;

    let cmd = Command {
        id: new_command_id(),
        device_id: device,
        action,
        timestamp: now_millis(),
        ttl: 8,
    };
    let reply = exchange(
        &node,
        &ch,
        &Message::Command(cmd.clone()),
        Duration::from_secs(3),
        |m| matches!(m, Message::Ack(a) if a.command_id == cmd.id),
    )
    .await?;

    match reply {
        Message::Ack(ack) => {
            println!(
                "Ack: result={}, new_state={}",
                ack.result_code,
                match &ack.new_state {
                    Some(s) => format!("on={:?} level={:?} value={:?}", s.on, s.level, s.value),
                    None => "-".into(),
                }
            );
            node.disconnect().await.ok();
            Ok(())
        }
        other => bail!("неожиданный ответ: {other:?}"),
    }
}

async fn query(peer: &Option<String>, scan_timeout: u64, devices: Vec<String>) -> Result<()> {
    let adapter = ble_adapter().await?;
    let node = find_node(&adapter, peer.as_deref(), Duration::from_secs(scan_timeout)).await?;
    println!("узел: {}", node.address());
    let ch = connect_and_find_char(&node).await?;

    let query = Query {
        device_ids: (!devices.is_empty()).then_some(devices),
        request_id: new_command_id(),
    };
    let reply = exchange(
        &node,
        &ch,
        &Message::Query(query.clone()),
        Duration::from_secs(3),
        |m| matches!(m, Message::Response(r) if r.request_id == query.request_id),
    )
    .await?;

    match reply {
        Message::Response(r) => {
            if r.devices.is_empty() {
                println!("устройств не найдено");
            }
            for (id, st) in r.devices {
                println!(
                    "{id}: on={:?} level={:?} value={:?}",
                    st.on, st.level, st.value
                );
            }
            node.disconnect().await.ok();
            Ok(())
        }
        other => bail!("неожиданный ответ: {other:?}"),
    }
}

async fn discover() -> Result<()> {
    println!("mDNS (_nucleus._tcp.local.):");
    if let Ok(daemon) = mdns_sd::ServiceDaemon::new() {
        if let Ok(receiver) = daemon.browse("_nucleus._tcp.local.") {
            let deadline = Instant::now() + Duration::from_secs(3);
            // recv_timeout по остатку дедлайна; 0 → таймаут → конец поиска
            while let Ok(event) =
                receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                    println!(
                        "  {} → [{}]:{}",
                        info.get_fullname(),
                        info.get_addresses()
                            .iter()
                            .map(|ip| ip.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                        info.get_port()
                    );
                }
            }
        }
        daemon.shutdown().ok();
    } else {
        println!("  (недоступен)");
    }

    println!("BLE (реклама «nu» / сервис Nucleus):");
    let adapter = ble_adapter().await?;
    adapter
        .start_scan(ScanFilter::default())
        .await
        .context("запуск сканирования")?;
    tokio::time::sleep(Duration::from_secs(5)).await;
    for p in adapter
        .peripherals()
        .await
        .context("перечисление устройств")?
    {
        if let Ok(Some(props)) = p.properties().await {
            let is_nucleus = props.local_name.as_deref() == Some("nu")
                || props
                    .services
                    .contains(&Uuid::parse_str(NUCLEUS_UUID_STR).unwrap());
            if is_nucleus {
                println!(
                    "  {} ({})",
                    p.address(),
                    props.local_name.unwrap_or_else(|| "?".into())
                );
            }
        }
    }
    Ok(())
}
