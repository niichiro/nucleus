
---

## 📄 `nucleus-mesh/README.md`

```markdown
# nucleus-mesh

Общая библиотека, реализующая протокол Nucleus для mesh-взаимодействия.  
Используется как на сервере (`nucleusd`), так и на микроконтроллерах (`nucleus-node`).

## Состав

- **Сообщения** — все типы пакетов Nucleus (Announce, Command, Ack, Query и др.)
- **Flooding-маршрутизация** — простая лавинная рассылка с TTL и кэшированием ID
- **Сериализация** — через `postcard` (компактный бинарный формат)
- **Криптография** (в будущем) — Ed25519 подписи и Noise-рукопожатия

## Использование

```rust
use nucleus_mesh::{Message, Announce, Command};

let msg = Message::Announce(Announce {
    node_id: my_id,
    node_type: NodeType::Esp32,
    devices: vec!["light.kitchen".into()],
});

let bytes = postcard::to_vec(&msg).unwrap();
// отправка через любой транспорт (BLE, ESP‑NOW)
