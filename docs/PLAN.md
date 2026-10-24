# План реализации Nucleus

План одобрен 2026-10-23; топология пересмотрена 2026-10-24 (Orange Pi исключена, сервер — домашний ПК). Живой документ. ТЗ — корневой `README.md`.

> ℹ️ Аппаратура в наличии: Zigbee-стик + 2× ESP32-C3 Super Mini (приехали по ошибке вместо заказанных C6). Решение (2026-10-24): остаёмся на C3 — их достаточно для всей узловой линии; C6 — точечная опция (Zigbee-роутер в дальних комнатах, локальный Zigbee без сервера в M4), докупается при реальных дырах покрытия. Orange Pi Zero 3 не заработала и исключена из схемы.

## Топология (2026-10-24)

```
телефон ──BLE──► ESP32-C3 #1 ──ESP-NOW──► ESP32-C3 #2
                                                │ BLE (ПК — централ, как телефон)
                                                ▼
  ПК: nucleusd ◄──► mosquitto ◄──► zigbee2mqtt ──► стик ──► Zigbee-устройства
           │
           └── состояние, логи, HTTP API, мониторинг
```

Принципы white-box (зафиксированы в README):
- ни одного дата-кабеля в эксплуатации: узлы — только питание, стик — в сервере;
- телефон — основной интерфейс; сервер — фоновый софт;
- обнаружение автоматическое (BLE-реклама, mDNS), без ввода адресов.

Опция на будущее: любой Linux-мини-сервер встаёт на место ПК без переделок —
nucleusd Linux-ready (feature `ble-peripheral`, bluer).

## Целевая структура

```
nucleus/
├── Cargo.toml              # workspace: crates/*; node/ и app/ исключены
├── crates/
│   ├── nucleus-core/       # no_std: протокол + движок (messages/codec/routing/transport/engine)
│   ├── nucleusd/           # сервер (Windows/Linux): ble-централ(M3), devices, state, http, mqtt, mdns
│   └── nucleus-ctl/        # CLI: BLE-централ + HTTP-режим (Windows/Linux/macOS)
├── node/                   # вне workspace: esp-idf прошивка ESP32-C3 (Фаза 2)
└── app/                    # вне workspace: UI (Фаза 3)
```

## Этапы

### M0 — Фундамент протокола ✅ (2026-10-23)
Слияние mesh+core → crates/nucleus-core; кадрирование `[magic, version, payload]`;
настраиваемый FloodRouter; TransportError; тесты (roundtrip, TTL/дедуп, in-memory
интеграция); CI + `[workspace.lints]` + thiserror v2.

### M1 — Фаза 0 «Виртуальный дом» ✅ код (2026-10-23)
nucleusd lib+bin (ble/config/devices/http/mqtt/mdns/state); DeviceExecutor в движке;
устройства (тогда виртуальные, state.json — удалены 2026-10-24); HTTP API + MQTT-мост;
rolling-логи + mDNS; nucleus-ctl (btleplug). Приёмка на Orange Pi отменена
(плата исключена). Виртуальные устройства удалены из кода 2026-10-24:
сервер управляет только реальными Zigbee-устройствами через zigbee2mqtt.

### M2 — Фаза 1 «Железо оживает» ✅ (принят 2026-10-24): Zigbee на ПК
1. nucleusd: bluer за cargo-feature `ble-peripheral` (по умолчанию off) — сервер
   собирается и работает на Windows.
2. `docs/DEPLOY-WINDOWS.md`: mosquitto + zigbee2mqtt (Node.js) нативно, конфиг стика.
3. Zigbee-устройства в DeviceManager: топики `zigbee2mqtt/bridge/devices` (список)
   и `zigbee2mqtt/<id>` (состояние); команды → `zigbee2mqtt/<id>/set`;
   маппинг exposed → Action; friendly_name = device_id.
4. nucleus-ctl: режим `--http <url>` — приёмка без BLE.

Приёмка (2026-10-24, пройдена): Tuya TS0505B_1 (RGB+CCT) спарена как `lamp:rgb`;
`ctl --http off/on/level` — лампа реагирует, реальное состояние возвращается отчётом
в `nucleus/lamp:rgb/state`; переименование через `bridge/request/device/rename`.
Живое тестирование поймало и исправило: лимит MQTT-пакета rumqttc (27 КБ bridge/devices),
фильтр служебной записи Coordinator.

### M3 — Фаза 2 «Настоящая mesh» (в работе с 2026-10-24): ESP32-C3
Платформа: разработка и отладка на C3 Super Mini; C6 (если будут докуплены) встаёт
в тот же код — тот же SDK, та же периферия, плюс радио 802.15.4 (Zigbee-роутер).
1. 🚧 `node/` — отдельный esp-idf workspace (каркас готов: esp-idf-svc + esp32-nimble +
   ESP-NOW FFI + движок nucleus-core, места APIs помечены TODO(build)); сборка требует
   установки esp-тулчейна (rustup target riscv32imc-esp-espidf, espflash, ldproxy;
   первая сборка качает ESP-IDF ~1.5 ГБ — с явного согласия пользователя).
2. ✅ nucleusd: транспорт `BleCentralTransport` (btleplug, feature `ble-central`) —
   ПК подключается к узлу как централ, с авто-переподключением; проверен clippy
   на Windows, живая проверка — с первой платой.
3. Приёмка: телефон/ctl ─BLE─► C3#1 ─ESP-NOW─► C3#2 ─BLE─► ПК ─► Zigbee-лампа,
   без единого дата-кабеля.
4. Мониторинг на ПК поверх HTTP API (полный UI — Фаза 3).

### M4 — Фаза 3 «Полная автономия»
Сценарии на узлах без сервера (компенсация выключаемого ПК; LWW по timestamp);
локальные Zigbee-сцены без сервера — только при наличии C6-узлов (у C3 нет радио 802.15.4);
OTA (подписанные образы); log push/pull; UI (Yew + Web Bluetooth или Tauri).

## Сквозные решения
- Всё ниже `Transport` тестируется на хосте; BLE e2e — вручную.
- Конфигурация: TOML + clap; MQTT-брокер опционален на этапе M2 (ретраи с warning).
- nucleusd на Windows = HTTP/MQTT/Zigbee; BLE-периферия (bluer) и BLE-централ
  (btleplug, M3) — независимые транспорты одного движка.
- Язык: проза (README, комментарии) — русская; идентификаторы и ошибки — английские.
