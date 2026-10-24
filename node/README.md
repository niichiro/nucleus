# nucleus-node

Прошивка узла Nucleus для **ESP32-C3 Super Mini**: BLE-периферия (телефон и ПК
подключаются как централы) + ESP-NOW-магистраль между узлами. Протокол и
flooding — движок `crates/nucleus-core`, тот же, что у сервера.

> Находится вне основного workspace: esp-idf собирается собственным тулчейном.
> Статус: каркас M3 — BLE и ESP-NOW помечены `TODO(build)`, включаются при
> первой сборке с платой в руках.

## Стек

- **Rust (esp-idf, std)** + `esp-idf-svc`
- **BLE:** NimBLE через `esp32-nimble` (GATT-профиль как у сервера: одна
  характеристика write+notify, кадры ≤ MTU)
- **ESP-NOW:** raw FFI `esp_idf_sys` (Wi-Fi STA + broadcast-flooding)
- **Движок:** `nucleus-core` (кадры `[magic, version, postcard]`, TTL/дедуп)

## Первая сборка (один раз)

```bash
rustup target add riscv32imc-esp-espidf
cargo install espflash ldproxy
cd node
cargo build --release
```

Первая сборка скачает ESP-IDF (~1.5 ГБ) и тулчейн riscv — это разовая
инициализация окружения, дальше инкрементально.

## Прошивка и монитор

```bash
cargo run --release          # espflash: выберет COM-порт платы, зальёт, откроет монитор
```

## Схема роли узла

```
телефон/ПК ──BLE──► этот узел ──ESP-NOW──► соседний узел ──BLE──► сервер(nucleusd)
```

Чужие команды узел ретранслирует (flooding с TTL и дедупом ID), своих
локальных устройств в M3 нет — GPIO-исполнители приходят в M4.
