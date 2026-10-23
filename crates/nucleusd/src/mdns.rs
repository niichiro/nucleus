// crates/nucleusd/src/mdns.rs

use std::net::IpAddr;

use mdns_sd::{ServiceDaemon, ServiceInfo};

/// Регистрирует сервис `_nucleus._tcp.local.` (HTTP API) в локальной сети.
///
/// Возвращает демон-хэндл: держать его живым всё время работы узла
/// (drop останавливает объявление).
pub fn register(instance: &str, port: u16) -> mdns_sd::Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;
    let ip = local_ip();
    let host_name = format!("{instance}.local.");
    let info = ServiceInfo::new("_nucleus._tcp.local.", instance, &host_name, ip, port, None)?;
    daemon.register(info)?;
    tracing::info!("mDNS: _nucleus._tcp.local. → {instance} [{ip}:{port}]");
    Ok(daemon)
}

/// Локальный IP без внешнего трафика: connect на TEST-NET адрес лишь выбирает
/// маршрут по умолчанию, пакеты не уходят. Фолбэк — loopback.
fn local_ip() -> IpAddr {
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("192.0.2.1:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip();
            }
        }
    }
    IpAddr::from([127, 0, 0, 1])
}
