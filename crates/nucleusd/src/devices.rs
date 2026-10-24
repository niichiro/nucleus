// crates/nucleusd/src/devices.rs

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nucleus_core::{Action, Command, DeviceExecutor, DeviceId, DeviceState, ExecuteError};
use rumqttc::{AsyncClient, QoS};
use serde::Deserialize;
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::broadcast;

/// JSON-тело команды для HTTP и MQTT: {"on": true} / {"level": 128} / {"value": 22.5}.
#[derive(Debug, Deserialize)]
pub struct SetBody {
    pub on: Option<bool>,
    pub level: Option<u8>,
    pub value: Option<f32>,
}

impl SetBody {
    pub fn to_action(&self) -> Option<Action> {
        self.on
            .map(Action::OnOff)
            .or_else(|| self.level.map(Action::SetLevel))
            .or_else(|| self.value.map(Action::SetValue))
    }
}

/// Устройство из zigbee2mqtt (friendly_name = device_id в протоколе Nucleus).
#[derive(Debug, Clone)]
pub struct ZigbeeDevice {
    pub friendly_name: String,
    /// "lamp" | "switch" | "sensor" — по exposed-свойствам.
    pub kind: String,
    pub supports_brightness: bool,
    /// Кэш последнего состояния от zigbee2mqtt.
    pub state: DeviceState,
}

/// Устройство узла в общем представлении (HTTP API, Query).
#[derive(Debug, Clone)]
pub struct AnyDevice {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source: &'static str, // "zigbee"
    pub state: DeviceState,
}

/// Событие изменения состояния устройства (подписчики: MQTT-мост, будущие сцены).
#[derive(Debug, Clone)]
pub struct DeviceEvent {
    pub device_id: String,
    pub state: DeviceState,
}

#[derive(Debug, Error)]
pub enum SetError {
    #[error("device {0} not found")]
    NotFound(String),
    #[error("action not supported by device {0}")]
    NotSupported(String),
    #[error("mqtt unavailable: {0}")]
    Mqtt(String),
}

/// Менеджер устройств сервера: устройства Zigbee из zigbee2mqtt (по MQTT).
///
/// Единая точка исполнения для всех каналов (BLE, HTTP, MQTT).
#[derive(Clone)]
pub struct DeviceManager {
    zigbee: Arc<Mutex<HashMap<String, ZigbeeDevice>>>,
    mqtt: Option<AsyncClient>,
    events: broadcast::Sender<DeviceEvent>,
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceManager {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            zigbee: Arc::new(Mutex::new(HashMap::new())),
            mqtt: None,
            events,
        }
    }

    /// MQTT-клиент для команд zigbee-устройствам; вызвать до клонирования менеджера.
    pub fn set_mqtt(&mut self, client: AsyncClient) {
        self.mqtt = Some(client);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DeviceEvent> {
        self.events.subscribe()
    }

    fn emit(&self, device_id: &str, state: &DeviceState) {
        let _ = self.events.send(DeviceEvent {
            device_id: device_id.to_string(),
            state: state.clone(),
        });
    }

    fn lock_zigbee(&self) -> std::sync::MutexGuard<'_, HashMap<String, ZigbeeDevice>> {
        self.zigbee.lock().expect("zigbee map poisoned")
    }

    /// Все устройства сервера.
    pub fn list(&self) -> Vec<AnyDevice> {
        self.lock_zigbee()
            .values()
            .map(|z| AnyDevice {
                id: z.friendly_name.clone(),
                name: z.friendly_name.clone(),
                kind: z.kind.clone(),
                source: "zigbee",
                state: z.state.clone(),
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<DeviceState> {
        self.lock_zigbee().get(id).map(|z| z.state.clone())
    }

    /// Применяет действие к zigbee-устройству (команда в zigbee2mqtt/<id>/set).
    pub async fn set(&self, id: &str, action: &Action) -> Result<DeviceState, SetError> {
        let device = self
            .lock_zigbee()
            .get(id)
            .cloned()
            .ok_or_else(|| SetError::NotFound(id.to_string()))?;
        let (payload, new_state) = zigbee_payload(&device, action)?;

        let Some(client) = &self.mqtt else {
            return Err(SetError::Mqtt("not configured".into()));
        };
        client
            .publish(
                format!("zigbee2mqtt/{id}/set"),
                QoS::AtLeastOnce,
                false,
                payload.to_string(),
            )
            .await
            .map_err(|e| SetError::Mqtt(e.to_string()))?;

        // Оптимистично: реальное состояние придёт топиком zigbee2mqtt/<id>
        self.lock_zigbee()
            .entry(id.to_string())
            .and_modify(|z| z.state = new_state.clone());
        self.emit(id, &new_state);
        Ok(new_state)
    }

    /// Обновляет список zigbee-устройств из `zigbee2mqtt/bridge/devices`.
    /// Кэш состояний существующих устройств сохраняется.
    pub fn update_zigbee_devices(&self, payload: &[u8]) {
        let parsed: Value = match serde_json::from_slice(payload) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("zigbee2mqtt/bridge/devices: {e}");
                return;
            }
        };
        let Some(arr) = parsed.as_array() else {
            return;
        };
        let old = self.lock_zigbee().clone();
        let mut fresh = HashMap::new();
        for device in arr {
            let Some(name) = device.get("friendly_name").and_then(Value::as_str) else {
                continue;
            };
            // Служебные записи: сам брокер (bridge*) и координатор
            if name.starts_with("bridge")
                || device.get("type").and_then(Value::as_str) == Some("Coordinator")
            {
                continue;
            }
            let supports_brightness = exposes_have(device, "brightness");
            let kind = if supports_brightness {
                "lamp"
            } else if exposes_have(device, "state") {
                "switch"
            } else {
                "sensor"
            };
            let state = old
                .get(name)
                .map(|z| z.state.clone())
                .unwrap_or(DeviceState {
                    on: None,
                    level: None,
                    value: None,
                });
            fresh.insert(
                name.to_string(),
                ZigbeeDevice {
                    friendly_name: name.to_string(),
                    kind: kind.to_string(),
                    supports_brightness,
                    state,
                },
            );
        }
        let count = fresh.len();
        *self.lock_zigbee() = fresh;
        tracing::info!("zigbee devices updated: {count}");
    }

    /// Обновляет кэш состояния zigbee-устройства из `zigbee2mqtt/<name>`.
    pub fn update_zigbee_state(&self, name: &str, payload: &[u8]) {
        let parsed: Value = match serde_json::from_slice(payload) {
            Ok(v) => v,
            Err(_) => return,
        };
        let new_state = {
            let mut zigbee = self.lock_zigbee();
            let Some(device) = zigbee.get_mut(name) else {
                return;
            };
            if let Some(s) = parsed.get("state").and_then(Value::as_str) {
                device.state.on = Some(!matches!(s, "OFF" | "off"));
            }
            if let Some(b) = parsed.get("brightness").and_then(Value::as_u64) {
                device.state.level = Some(b.min(255) as u8);
            }
            device.state.clone()
        };
        self.emit(name, &new_state);
    }
}

/// JSON-команда zigbee2mqtt и оптимистичное новое состояние.
fn zigbee_payload(
    device: &ZigbeeDevice,
    action: &Action,
) -> Result<(Value, DeviceState), SetError> {
    let mut state = device.state.clone();
    match action {
        Action::OnOff(v) => {
            state.on = Some(*v);
            Ok((json!({"state": if *v { "ON" } else { "OFF" }}), state))
        }
        Action::SetLevel(level) if device.supports_brightness => {
            let level = (*level).min(254); // zigbee: 0-254
            state.level = Some(level);
            Ok((json!({"brightness": level}), state))
        }
        Action::SetLevel(_) | Action::SetValue(_) => {
            Err(SetError::NotSupported(device.friendly_name.clone()))
        }
    }
}

/// Есть ли свойство (state/brightness) в exposed-описании устройства.
/// Схема zigbee2mqtt гуляет между версиями — ищем имя по дереву definition.
fn exposes_have(device: &Value, needle: &str) -> bool {
    device
        .get("definition")
        .map(|def| walk_exposes(def, needle))
        .unwrap_or(false)
}

fn walk_exposes(value: &Value, needle: &str) -> bool {
    match value {
        Value::Array(items) => items.iter().any(|item| walk_exposes(item, needle)),
        Value::Object(map) => map.iter().any(|(key, val)| {
            (key == "name" || key == "property") && val.as_str() == Some(needle)
                || walk_exposes(val, needle)
        }),
        _ => false,
    }
}

#[async_trait]
impl DeviceExecutor for DeviceManager {
    async fn execute(&mut self, command: &Command) -> Result<DeviceState, ExecuteError> {
        self.set(&command.device_id, &command.action)
            .await
            .map_err(|e| match e {
                SetError::NotFound(_) => ExecuteError::DeviceNotFound,
                SetError::NotSupported(what) => {
                    ExecuteError::Failed(format!("not supported: {what}"))
                }
                SetError::Mqtt(m) => ExecuteError::Failed(format!("mqtt: {m}")),
            })
    }

    async fn states(&self, device_ids: Option<&[DeviceId]>) -> Vec<(DeviceId, DeviceState)> {
        match device_ids {
            None => self.list().into_iter().map(|d| (d.id, d.state)).collect(),
            Some(ids) => ids
                .iter()
                .filter_map(|id| self.get(id).map(|s| (id.clone(), s)))
                .collect(),
        }
    }
}
