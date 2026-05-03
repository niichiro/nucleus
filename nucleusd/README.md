# nucleusd

Серверная часть Nucleus для Orange Pi (или любого Linux-хоста с BLE и Zigbee-стиком).  
Обеспечивает:

- BLE-периферию для приёма команд от телефонов и ESP32
- Координацию Zigbee-сети (через `zigbee2mqtt` и MQTT)
- Хранение состояния устройств и rolling-логов
- API для отладки (HTTP + WebSocket) и mDNS-анонс

## Стек

- **Язык:** Rust (edition 2021)
- **Асинхронный рантайм:** `tokio`
- **BLE:** `bluer` (Linux/BlueZ)
- **MQTT:** `rumqttc`
- **Логи:** `tracing` + `tracing-appender`
- **mDNS:** `mdns-sd`

## Быстрый старт

```bash
# Собрать
cargo build --release -p nucleusd
# Запустить (требуются права на BLE и, возможно, root)
sudo ./target/release/nucleusd
