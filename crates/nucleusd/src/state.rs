// crates/nucleusd/src/state.rs

use std::{collections::HashMap, fs, io, path::PathBuf};

use nucleus_core::{Action, DeviceState};
use serde::{Deserialize, Serialize};

/// Запись устройства в state.json (формат совместим с исходным файлом).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEntry {
    /// Идентификатор протокола (например, "lamp:living").
    pub id: String,
    /// Тип устройства ("lamp", "switch", ...).
    pub kind: String,
    /// Человеческое имя.
    pub name: String,
    /// Состояние (для лампы).
    pub state: LampState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LampState {
    pub on: bool,
    pub brightness: u8,
}

impl DeviceEntry {
    /// Представление состояния в протоколе Nucleus.
    pub fn device_state(&self) -> DeviceState {
        DeviceState {
            on: Some(self.state.on),
            level: Some(self.state.brightness),
            value: None,
        }
    }
}

/// Файл состояния узла.
#[derive(Debug, Default, Serialize, Deserialize)]
struct StateFile {
    #[serde(default)]
    devices: HashMap<String, DeviceEntry>,
    /// Сцены храним как есть (логика сцен — Фаза 3).
    #[serde(default)]
    scenes: serde_json::Value,
}

/// Хранилище состояния: state.json в каталоге данных, атомарное сохранение.
pub struct StateStore {
    path: PathBuf,
    file: StateFile,
}

impl StateStore {
    /// Загружает состояние; отсутствующий или битый файл — не ошибка,
    /// узел стартует с пустым набором устройств.
    pub fn load_or_default(path: PathBuf) -> StateStore {
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(file) => StateStore { path, file },
                Err(e) => {
                    tracing::warn!("state file {path:?} corrupted ({e}), starting empty");
                    StateStore {
                        path,
                        file: StateFile::default(),
                    }
                }
            },
            Err(_) => {
                tracing::info!("no state file at {path:?}, starting empty");
                StateStore {
                    path,
                    file: StateFile::default(),
                }
            }
        }
    }

    /// Все устройства узла.
    pub fn list(&self) -> Vec<DeviceEntry> {
        self.file.devices.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<DeviceState> {
        self.file
            .devices
            .values()
            .find(|d| d.id == id)
            .map(DeviceEntry::device_state)
    }

    /// Применяет действие к устройству, сохраняет состояние.
    /// None — устройство не найдено.
    pub fn apply(&mut self, id: &str, action: &Action) -> Option<DeviceState> {
        let entry = self.file.devices.values_mut().find(|d| d.id == id)?;
        match action {
            Action::OnOff(v) => entry.state.on = *v,
            Action::SetLevel(l) => entry.state.brightness = *l,
            // Числовые значения лампам не применимы — состояние не меняем
            Action::SetValue(_) => {}
        }
        let new_state = Self::to_device_state(entry);
        self.save();
        Some(new_state)
    }

    /// Атомарное сохранение: временный файл + rename.
    fn save(&self) {
        if let Err(e) = self.save_result() {
            tracing::error!("failed to save state to {:?}: {e}", self.path);
        }
    }

    fn save_result(&self) -> io::Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&self.file)?)?;
        fs::rename(&tmp, &self.path)
    }
}
