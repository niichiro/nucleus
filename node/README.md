# nucleus-node

Прошивка для микроконтроллеров ESP32-C6, превращающая их в полноправные узлы mesh-сети Nucleus.

> Находится вне основного workspace: собирается отдельным esp-idf workspace (Фаза 2, см. `docs/PLAN.md`).

Возможности:

- Приём команд по BLE от телефона или Orange Pi
- Обмен сообщениями с другими ESP32 через **ESP-NOW**
- Лавинная маршрутизация (flooding) с кешированием ID
- Прямое управление Zigbee-устройствами (встроенный радиочип) или через GPIO
- Автономная работа при отсутствии Orange Pi (локальные сценарии)

## Стек

- **Rust (no_std)** + `esp-idf-hal`
- **Wi-Fi/BLE:** `esp-wifi`, `esp32-nimble`
- **ESP-NOW:** `esp-now-rs`
- **Протокол и маршрутизация:** крейт `nucleus-core` (общий с `nucleusd`)

## Сборка и прошивка

После перевода на отдельный esp-idf workspace (Фаза 2):

```bash
# Установить тулчейн для RISC-V
rustup target add riscv32imac-unknown-none-elf
cargo install espflash ldproxy

# Собрать и прошить
cargo run --release
```
