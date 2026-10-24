// crates/nucleusd/src/mqtt.rs

use std::time::Duration;

use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::devices::{DeviceEvent, DeviceManager, SetBody};

/// Создаёт MQTT-клиента и цикл событий (само подключение — на первом poll).
pub fn connect(broker: &str) -> (AsyncClient, EventLoop) {
    let (host, port) = parse_broker(broker);
    let mut options = MqttOptions::new("nucleusd", host, port);
    options.set_keep_alive(Duration::from_secs(30));
    AsyncClient::new(options, 64)
}

/// MQTT-цикл сервера.
///
/// Подписки: `nucleus/+/set` (команды), `zigbee2mqtt/#` (устройства и состояния).
/// Публикации: `nucleus/{id}/state` при каждом изменении состояния (любым каналом).
///
/// Брокер опционален: при недоступности — warning и переподключение,
/// HTTP и виртуальные устройства работают без него.
pub async fn run(manager: DeviceManager, client: AsyncClient, mut eventloop: EventLoop) {
    for topic in ["nucleus/+/set", "zigbee2mqtt/#"] {
        if let Err(e) = client.subscribe(topic, QoS::AtLeastOnce).await {
            tracing::warn!("MQTT subscribe {topic}: {e}");
        }
    }
    tracing::info!("MQTT bridge: nucleus/+/set, zigbee2mqtt/#");

    let mut events = manager.subscribe();
    loop {
        tokio::select! {
            incoming = eventloop.poll() => match incoming {
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    route(&manager, &publish).await;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("MQTT: {e}, retrying in 5s");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            },
            event = events.recv() => match event {
                Ok(DeviceEvent { device_id, state }) => {
                    let topic = format!("nucleus/{device_id}/state");
                    let payload = json!({
                        "on": state.on,
                        "level": state.level,
                        "value": state.value,
                    });
                    if let Err(e) = client
                        .publish(&topic, QoS::AtLeastOnce, false, payload.to_string())
                        .await
                    {
                        tracing::warn!("MQTT publish {topic}: {e}");
                    }
                }
                Err(RecvError::Lagged(n)) => tracing::warn!("MQTT: skipped {n} device events"),
                Err(RecvError::Closed) => return,
            },
        }
    }
}

/// Маршрутизация входящих публикаций по префиксу топика.
async fn route(manager: &DeviceManager, publish: &rumqttc::Publish) {
    let topic = publish.topic.as_str();
    if let Some(device_id) = topic
        .strip_prefix("nucleus/")
        .and_then(|t| t.strip_suffix("/set"))
    {
        handle_command(manager, device_id, publish).await;
    } else if topic == "zigbee2mqtt/bridge/devices" {
        manager.update_zigbee_devices(&publish.payload);
    } else if let Some(name) = topic.strip_prefix("zigbee2mqtt/") {
        if !name.starts_with("bridge") {
            manager.update_zigbee_state(name, &publish.payload);
        }
    }
}

async fn handle_command(manager: &DeviceManager, device_id: &str, publish: &rumqttc::Publish) {
    let Ok(body) = serde_json::from_slice::<SetBody>(&publish.payload) else {
        tracing::warn!(
            "MQTT: invalid JSON for {device_id}: {}",
            String::from_utf8_lossy(&publish.payload)
        );
        return;
    };
    let Some(action) = body.to_action() else {
        tracing::warn!("MQTT: no action for {device_id}");
        return;
    };
    match manager.set(device_id, &action).await {
        Ok(_) => tracing::debug!("MQTT: applied {action:?} to {device_id}"),
        Err(e) => tracing::warn!("MQTT: {e}"),
    }
}

fn parse_broker(broker: &str) -> (String, u16) {
    match broker.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().unwrap_or(1883)),
        None => (broker.to_string(), 1883),
    }
}
