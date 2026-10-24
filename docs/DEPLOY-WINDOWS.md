# Развёртывание сервера Nucleus на Windows (ПК)

Сервер = nucleusd (HTTP/MQTT/состояние) + mosquitto (брокер) + zigbee2mqtt (Zigbee-стик).
BLE на сервере не нужен: узлы ESP32 — BLE-периферии, ПК в M3 подключится к ним как централ.

## 1. Сборка nucleusd

Из корня репозитория (Git Bash или PowerShell):

```bash
cargo build --release -p nucleusd          # без BLE — работает на Windows
cargo run --release -p nucleusd -- --data-dir "G:\Soft\nucleus-data" --http-port 3000
```

Флаги: `--data-dir` (логи), `--config nucleusd.toml`, `--mqtt-broker 127.0.0.1:1883`, `--no-mdns`.

Проверка: `curl http://localhost:3000/api/devices` — список устройств.

## 2. mosquitto (MQTT-брокер)

```bash
winget install --id EclipseFoundation.Mosquitto --silent
```

Сервис ставится и стартует сам; правка конфига не нужна — локальные (localhost)
анонимные подключения разрешены из коробки, а всё живёт на одном ПК.

Проверка roundtrip (sub в фоне → pub):

```bash
M="/c/Program Files/mosquitto"
($M/mosquitto_sub.exe -h localhost -t 'nucleus/#' -v -C 1 & sleep 1; $M/mosquitto_pub.exe -h localhost -t nucleus/test -m ok)
```

## 3. zigbee2mqtt + Zigbee-стик

1. Node.js ≥ 18 (уже стоит? `node --version`).
2. `git clone --depth 1 https://github.com/Koenkk/zigbee2mqtt.git G:\Soft\zigbee2mqtt && cd G:\Soft\zigbee2mqtt && npm install`
3. Разрешить нативный serialport: `npm approve-scripts @serialport/bindings-cpp`.
4. `G:\Soft\zigbee2mqtt\data\configuration.yaml`:

```yaml
mqtt:
  server: mqtt://localhost:1883
serial:
  adapter: ember        # Sonoff ZBDongle-Plus-E; TI-стики → zstack, ConBee → conbee
  port: COM3            # номер порта стика (Диспетчер устройств → Порты)
frontend:
  enabled: true         # веб-морда http://localhost:8080 — спаривание и переименование
permit_join: false      # включать true только на время добавления устройств
```

5. Запуск: `cd G:\Soft\zigbee2mqtt && npm start`.

## 4. Связка

- zigbee2mqtt публикует устройства в `zigbee2mqtt/bridge/devices` — nucleusd подхватит их автоматически
  (лог: `zigbee devices updated: N`).
- **friendly_name в zigbee2mqtt = device_id в Nucleus** — назовите устройства удобно
  (напр. `lamp:hall`) в веб-морде zigbee2mqtt.
- Состояния Zigbee-устройств прилетают в `nucleus/<id>/state`, команды — `nucleus/<id>/set`
  и `zigbee2mqtt/<id>/set`.

## 5. Приёмка M2 (чеклист)

```bash
# реальное Zigbee-устройство (после спаривания в веб-морде zigbee2mqtt)
cargo run --release -p nucleus-ctl -- --http http://localhost:3000 on <zigbee-id>
cargo run --release -p nucleus-ctl -- --http http://localhost:3000 query
```

- [ ] Zigbee-устройство физически реагирует, состояние видно в `query` и веб-морде
- [ ] `mosquitto_sub -t 'nucleus/#' -v` показывает изменения из всех каналов
- [ ] nucleusd работает без брокера/zigbee2mqtt (warnings в логе, HTTP живой)

## 6. Автостарт (постоянная эксплуатация)

nssm (nssm.cc) или Планировщик заданий: службы `nucleusd`, `mosquitto`, `zigbee2mqtt`
(zigbee2mqtt: `nssm install zigbee2mqtt "C:\Program Files\nodejs\npm.cmd" "start"`, рабочая папка `G:\Soft\zigbee2mqtt`).

## Замечания

- ПК-сервер выключен → Zigbee-канал недоступен; mesh (M3) продолжит работать — это осознанный компромисс до M4.
- Тип стика (adapter) и порт COM смотрите в Диспетчере устройств; частые адаптеры:
  Sonoff ZBDongle-E/P → `ezsp`/`zstack`, ConBee II/III → `conbee`, SLZB-06 → `ember`.
