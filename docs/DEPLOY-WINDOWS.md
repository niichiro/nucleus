# Развёртывание сервера Nucleus на Windows (ПК)

Сервер = nucleusd (HTTP/MQTT/состояние) + mosquitto (брокер) + zigbee2mqtt (Zigbee-стик).
BLE на сервере не нужен: узлы ESP32 — BLE-периферии, ПК в M3 подключится к ним как централ.

## 1. Сборка nucleusd

Из корня репозитория (Git Bash или PowerShell):

```bash
cargo build --release -p nucleusd          # без BLE — работает на Windows
cargo run --release -p nucleusd -- --data-dir "C:\nucleus" --http-port 3000
```

Флаги: `--data-dir` (state.json + логи), `--config nucleusd.toml`, `--mqtt-broker 127.0.0.1:1883`, `--no-mdns`.
При первом старте положите `crates/nucleusd/state.json` в каталог данных (виртуальные лампы для теста).

Проверка: `curl http://localhost:3000/api/devices` — список устройств.

## 2. mosquitto (MQTT-брокер)

1. Скачать и установить mosquitto (mosquitto.org, в установщике отметить «service»).
2. `C:\Program Files\mosquitto\mosquitto.conf` — добавить:

```
listener 1883
allow_anonymous true
```

3. Перезапустить службу: `net stop mosquitto && net start mosquitto` (от администратора).

Проверка: `mosquitto_sub -t 'nucleus/#' -v` — при изменении лампы появятся `nucleus/lamp:living/state`.

## 3. zigbee2mqtt + Zigbee-стик

1. Установить Node.js LTS (nodejs.org).
2. Скачать zigbee2mqtt (zip с github.com/Koenkk/zigbee2mqtt) и распаковать, например, в `C:\zigbee2mqtt`.
3. `C:\zigbee2mqtt\data\configuration.yaml`:

```yaml
mqtt:
  server: mqtt://localhost:1883
serial:
  adapter: zstack      # подберите по стику: см. «Замечания» внизу
  port: COM3           # номер порта стика (Диспетчер устройств → Порты)
frontend:
  enabled: true        # веб-морда http://localhost:8080 — спаривание и переименование
permit_join: false     # включать true только на время добавления устройств
```

4. Запуск: `cd C:\zigbee2mqtt && npm start`.

## 4. Связка

- zigbee2mqtt публикует устройства в `zigbee2mqtt/bridge/devices` — nucleusd подхватит их автоматически
  (лог: `zigbee devices updated: N`).
- **friendly_name в zigbee2mqtt = device_id в Nucleus** — назовите устройства удобно
  (напр. `lamp:hall`) в веб-морде zigbee2mqtt.
- Состояния Zigbee-устройств прилетают в `nucleus/<id>/state`, команды — `nucleus/<id>/set`
  и `zigbee2mqtt/<id>/set`.

## 5. Приёмка M2 (чеклист)

```bash
# виртуальная лампа (state.json)
cargo run --release -p nucleus-ctl -- --http http://localhost:3000 on lamp:living

# реальное Zigbee-устройство (после спаривания в веб-морде zigbee2mqtt)
cargo run --release -p nucleus-ctl -- --http http://localhost:3000 on <zigbee-id>
cargo run --release -p nucleus-ctl -- --http http://localhost:3000 query
```

- [ ] Zigbee-устройство физически реагирует, состояние видно в `query` и веб-морде
- [ ] Состояние виртуальных устройств переживает рестарт nucleusd (state.json)
- [ ] `mosquitto_sub -t 'nucleus/#' -v` показывает изменения из всех каналов
- [ ] nucleusd работает без брокера/zigbee2mqtt (warnings в логе, HTTP живой)

## 6. Автостарт (постоянная эксплуатация)

nssm (nssm.cc) или Планировщик заданий: службы `nucleusd`, `mosquitto`, `zigbee2mqtt`
(zigbee2mqtt: `nssm install zigbee2mqtt "C:\Program Files\nodejs\npm.cmd" "start"`, рабочая папка `C:\zigbee2mqtt`).

## Замечания

- ПК-сервер выключен → Zigbee-канал недоступен; mesh (M3) продолжит работать — это осознанный компромисс до M4.
- Тип стика (adapter) и порт COM смотрите в Диспетчере устройств; частые адаптеры:
  Sonoff ZBDongle-E/P → `ezsp`/`zstack`, ConBee II/III → `conbee`, SLZB-06 → `ember`.
