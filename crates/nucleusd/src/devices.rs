// crates/nucleusd/src/devices.rs

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nucleus_core::{Action, Command, DeviceExecutor, DeviceId, DeviceState, ExecuteError};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::broadcast;

use crate::state::{DeviceEntry, StateStore};

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
}

/// Менеджер устройств узла: виртуальные устройства из state.json.
///
/// Единая точка исполнения для всех каналов (BLE, HTTP, MQTT); в M2 сюда
/// добавится ветка Zigbee. Реализует протокольный [`DeviceExecutor`].
#[derive(Clone)]
pub struct DeviceManager {
    store: Arc<Mutex<StateStore>>,
    events: broadcast::Sender<DeviceEvent>,
}

impl DeviceManager {
    pub fn new(store: StateStore) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            store: Arc::new(Mutex::new(store)),
            events,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DeviceEvent> {
        self.events.subscribe()
    }

    /// Все устройства узла (для HTTP API).
    pub fn entries(&self) -> Vec<DeviceEntry> {
        self.lock().list()
    }

    pub fn get(&self, id: &str) -> Option<DeviceState> {
        self.lock().get(id)
    }

    /// Применяет действие, сохраняет состояние, рассылает событие.
    pub fn set(&self, id: &str, action: &Action) -> Result<DeviceState, SetError> {
        let new_state = self
            .lock()
            .apply(id, action)
            .ok_or_else(|| SetError::NotFound(id.to_string()))?;
        let _ = self.events.send(DeviceEvent {
            device_id: id.to_string(),
            state: new_state.clone(),
        });
        Ok(new_state)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StateStore> {
        self.store.lock().expect("state store poisoned")
    }
}

#[async_trait]
impl DeviceExecutor for DeviceManager {
    async fn execute(&mut self, command: &Command) -> Result<DeviceState, ExecuteError> {
        self.set(&command.device_id, &command.action)
            .map_err(|e| match e {
                SetError::NotFound(_) => ExecuteError::DeviceNotFound,
            })
    }

    async fn states(&self, device_ids: Option<&[DeviceId]>) -> Vec<(DeviceId, DeviceState)> {
        match device_ids {
            None => self
                .entries()
                .into_iter()
                .map(|e| (e.id, e.device_state()))
                .collect(),
            Some(ids) => ids
                .iter()
                .filter_map(|id| self.get(id).map(|s| (id.clone(), s)))
                .collect(),
        }
    }
}
