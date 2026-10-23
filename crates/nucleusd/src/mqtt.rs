// crates/nucleusd/src/mqtt.rs

use std::time::Duration;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::devices::{DeviceEvent, DeviceManager, SetBody};

/// MQTT-мост.
///
/// Команды:   топик `nucleus/{device_id}/set`, JSON как у HTTP ({"on": true}).
/// Состояния: `nucleus/{device_id}/state` при каждом изменении (любым каналом).
///
/// Брокер опционален: при недоступности пишет warning и переподключается,
/// демон продолжает работать (BLE/HTTP не зависят от MQTT).
pub async fn run(manager: DeviceManager, broker: &str) {
    let (host, port) = parse_broker(broker);
    let mut options = MqttOptions::new("nucleusd", host, port);
    options.set_keep_alive(Duration::from_secs(30));

    let (client, mut eventloop) = AsyncClient::new(options, 16);
    if let Err(e) = client.subscribe("nucleus/+/set", QoS::AtLeastOnce).await {
        tracing::warn!("MQTT subscribe failed: {e}");
    }
    tracing::info!("MQTT bridge to {broker} (nucleus/+/set ↔ nucleus/+/state)");

    let mut events = manager.subscribe();
    loop {
        tokio::select! {
            incoming = eventloop.poll() => match incoming {
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    handle_publish(&client, &manager, &publish).await;
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
                    if let Err(e) = client.publish(&topic, QoS::AtLeastOnce, false, payload.to_string()).await {
                        tracing::warn!("MQTT publish {topic}: {e}");
                    }
                }
                Err(RecvError::Lagged(n)) => tracing::warn!("MQTT: skipped {n} device events"),
                Err(RecvError::Closed) => return,
            },
        }
    }
}

async fn handle_publish(client: &AsyncClient, manager: &DeviceManager, publish: &rumqttc::Publish) {
    let Some(device_id) = publish
        .topic
        .strip_prefix("nucleus/")
        .and_then(|t| t.strip_suffix("/set"))
    else {
        return;
    };
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
    match manager.set(device_id, &action) {
        Ok(_) => tracing::debug!("MQTT: applied {action:?} to {device_id}"),
        Err(e) => tracing::warn!("MQTT: {e}"),
    }
    let _ = client; // публикация состояния придёт через DeviceEvent
}

fn parse_broker(broker: &str) -> (String, u16) {
    match broker.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().unwrap_or(1883)),
        None => (broker.to_string(), 1883),
    }
}
