# План реализации Nucleus

Зафиксированная версия плана (одобрен 2026-10-23). Живой документ: обновлять по мере прохождения этапов. ТЗ — корневой `README.md`.

> ℹ️ Вся аппаратура в наличии (2026-10): Zigbee-стик и ESP32-C6 получены — M2/M3 не привязаны к поставкам. Блокер только один: приёмка M1 на реальном Orange Pi.

## Целевая структура

```
nucleus/
├── Cargo.toml              # workspace: crates/*; node/ и app/ исключены
├── crates/
│   ├── nucleus-core/       # no_std: протокол + движок (слияние mesh + core)
│   └── nucleusd/           # демон Orange Pi (lib + bin, Фаза 0)
├── node/                   # вне workspace: esp-idf прошивка ESP32-C6 (Фаза 2)
└── app/                    # вне workspace: Yew/WebBluetooth UI (Фаза 3)
```

## Этапы

### M0 — Фундамент протокола ✅ (завершён 2026-10-23)
- Слияние nucleus-mesh + nucleus-core → crates/nucleus-core; вынос nucleus-node → node/, nucleus-app → app/.
- Кадрирование `[magic, version, payload]` поверх postcard; несовпадение версии = drop + лог.
- Настраиваемые LRU/TTL в FloodRouter; реальный TransportError.
- Тесты: roundtrip, TTL/дедуп, версионирование, интеграция двух движков через in-memory Transport.
- CI (GitHub Actions): fmt, clippy -D warnings, test. `[workspace.lints]`, thiserror v2.

### M1 — Фаза 0 «Виртуальный дом» ✅ (реализован 2026-10-23; приёмка на Orange Pi — по деплою)
- Рефакторинг nucleusd: lib + тонкий bin, модули ble/config/devices/http/mdns/mqtt/state.
- Трейт `DeviceExecutor` в nucleus-core: Ack несёт new_state, Query идёт через executor.
- Виртуальные устройства (state.json, формат сохранён), атомарный снапшот (tmp+rename), `--data-dir`.
- HTTP API (axum 0.8): GET /api/devices, GET/POST /api/devices/{id}/set.
- MQTT-мост (rumqttc): `nucleus/{id}/set` ↔ `nucleus/{id}/state`, живёт с ретраями.
- BLE: write для Command, notify для Ack через CharacteristicWriter; сессии чтения переживают несколько записей.
- Rolling-логи (tracing-appender, сутки) + консоль; mDNS `_nucleus._tcp.local.`.
- Новый крейт **nucleus-ctl** (btleplug, Windows/Linux/macOS): `discover`, `on/off/level`, `query`.
- Удалены models.rs и старые JSON-типы. systemd-юнит `deploy/nucleusd.service`, инструкция `docs/DEPLOY.md`.

Приёмка: с ноутбука nucleus-ctl включает виртуальную лампу на Orange Pi по BLE, видит Ack с новым состоянием; состояние переживает рестарт; демон находится по mDNS; логи ротируются.

### M2 — Фаза 1 «Железо оживает» (следующий этап; Zigbee-стик получен 2026-10)
- zigbee-драйвер `DeviceDriver` поверх MQTT-топиков zigbee2mqtt.
- Реконсиляция состояния при старте (Query реальных состояний → снапшот).

Приёмка: телефон → BLE → nucleusd → MQTT → zigbee2mqtt → физическое устройство; Ack возвращает фактическое состояние.

### M3 — Фаза 2 «Настоящая mesh» (ESP32-C6 получены 2026-10; старт после M2)
- node/ как отдельный esp-idf workspace: ESP-NOW-транспорт (кадры ≤ 250 байт) + BLE-периферия (NimBLE), движок из nucleus-core.
- Роли зафиксированы: Orange Pi всегда BLE-периферия; телефоны и ESP32 — централы.
- Flooding на железе: дедуп по id, TTL. Подписи Ed25519 (snow), анти-replay (timestamp + дедуп).

Приёмка: цепочка «телефон → ESP32 → ESP-NOW → … → Orange Pi → Zigbee-устройство».

### M4 — Фаза 3 «Полная автономия»
- Сценарии: локальные на OP → распределённые на ESP32 без OP; синхронизация Query/Response, конфликты — LWW по timestamp.
- OTA: сообщения Reboot/OTA, esp_ota, подписанные образы.
- Log push/pull полностью: критичные события сразу, журналы по запросу (OP — хранитель).
- UI: app/ (Yew + Web Bluetooth) или пересмотр (iOS Web Bluetooth не поддерживает).

## Сквозные решения
- Всё ниже `Transport` тестируется на хосте с моками; BLE e2e — вручную (в CI BlueZ нет).
- Конфигурация: TOML + clap; dotenvy убран.
- Из зависимостей nucleusd удалены: dbus*, libc, tower, dotenvy, anyhow, pin-project, tokio-stream.
- Разработка с Windows: протокол/движок — локально; демон — на Orange Pi или Linux (WSL2 для BLE не годится).
- Язык: проза (README, комментарии) — русская; идентификаторы и тексты ошибок — английские.
