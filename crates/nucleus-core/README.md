# nucleus-core

Общее ядро Nucleus (no_std): протокол и движок узла.
Используется демоном `nucleusd` на Orange Pi и будущей прошивкой `node/` на ESP32-C6.

## Состав

- **Сообщения** — типы пакетов Nucleus (Announce, Command, Ack, Query, Response)
- **Кодек** — кадр `[magic "NUC", version, postcard-payload]`; несовместимая версия протокола = drop кадра на приёме
- **Flooding-маршрутизация** — лавинная рассылка с TTL и дедупликацией ID команд (LRU)
- **Транспорт** — трейт `Transport` (BLE, ESP-NOW, in-memory в тестах)
- **Движок** — `NucleusCore<T: Transport>`: исполнение локальных команд, Ack, ретрансляция чужих, Query/Response
- **Криптография** (Фаза 2+) — Ed25519-подписи и Noise-рукопожатия (`snow`)

## Использование

```rust
use nucleus_core::{codec, Action, Command, Message, NucleusCore};

let frame = codec::encode(&Message::Command(command))?;
let msg: Message = codec::decode(&frame)?;
```

Сборка и тесты на хосте:

```bash
cargo test -p nucleus-core
```
