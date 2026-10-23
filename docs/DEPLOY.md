# Деплой nucleusd на Orange Pi

## Сборка (на самом Orange Pi)

```bash
sudo apt update
sudo apt install -y libdbus-1-dev pkg-config libfontconfig1-dev rustup  # rustup: https://rustup.rs
git clone https://github.com/niichiro/nucleus.git && cd nucleus
cargo build --release -p nucleusd
sudo install -Dm755 target/release/nucleusd /usr/local/bin/nucleusd
```

## Служба systemd

```bash
sudo useradd -r -d /var/lib/nucleus nucleus || true
sudo mkdir -p /var/lib/nucleus
# при первом старте положите устройства в /var/lib/nucleus/state.json (см. state.json в репо)
sudo cp deploy/nucleusd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now nucleusd
journalctl -u nucleusd -f
```

Логи: `journalctl` (консоль) + суточные файлы `/var/lib/nucleus/logs/nucleusd.log.*`.
Состояние: `/var/lib/nucleus/state.json` (атомарная запись при каждом изменении).

## MQTT (опционально, нужен для Фазы 1 / zigbee2mqtt)

```bash
sudo apt install -y mosquitto mosquitto-clients
mosquitto_sub -t 'nucleus/+/state' -v
mosquitto_pub -t 'nucleus/lamp:living/set' -m '{"on": true}'
```

Без брокера демон работает (BLE/HTTP не зависят от MQTT) — в логах будет warning с ретраями.

## Проверка

- HTTP: `curl http://<ip-pi>:3000/api/devices` (mDNS: `_nucleus._tcp.local.`)
- BLE с ноутбука (Windows/Linux/macOS): `cargo run -p nucleus-ctl -- on lamp:living`

## Замечания

- BlueZ должен быть включён (`bluetoothctl show` — Powered: yes), демон сам включает адаптер.
- Роли: Orange Pi всегда BLE-**периферия**; ноутбук/телефон/ESP32 — централы.
