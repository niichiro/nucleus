// nucleus-mesh/src/messages.rs

use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

/// Уникальный идентификатор узла (публичный ключ Ed25519 или MAC-адрес).
pub type NodeId = [u8; 32];

/// Идентификатор устройства (например, "light.living_room").
pub type DeviceId = String;

/// Тип узла Nucleus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    OrangePi,
    Esp32,
    Phone,
    Laptop,
}

/// Действие над устройством.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Включить / выключить.
    OnOff(bool),
    /// Установить уровень (яркость 0-255, громкость, процент).
    SetLevel(u8),
    /// Установить числовое значение (температура, влажность).
    SetValue(f32),
}

/// Сообщение протокола Nucleus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Периодическое объявление узла (heartbeat / announce).
    Announce(Announce),
    /// Команда управления устройством.
    Command(Command),
    /// Подтверждение выполнения команды.
    Ack(Ack),
    /// Запрос состояния устройств.
    Query(Query),
    /// Ответ на запрос состояния.
    Response(Response),
}

/// Объявление узла.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announce {
    /// Уникальный ID узла (публичный ключ или MAC).
    pub node_id: NodeId,
    /// Тип узла.
    pub node_type: NodeType,
    /// Флаги (например, Zigbee-координатор, питание от сети).
    pub flags: u8,
    /// Список устройств, которыми узел управляет напрямую.
    pub devices: Vec<DeviceId>,
    /// Версия протокола (для совместимости).
    pub protocol_version: u8,
}

impl Announce {
    pub fn new(
        node_id: NodeId,
        node_type: NodeType,
        flags: u8,
        devices: Vec<DeviceId>,
        protocol_version: u8,
    ) -> Result<Announce, CreateAnnounceError> {
        Ok(Announce {
            node_id,
            node_type,
            flags,
            devices,
            protocol_version,
        })
    }
}

/// Команда управления устройством.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Уникальный идентификатор команды (защита от дублирования).
    pub id: u32,
    /// Идентификатор устройства-получателя.
    pub device_id: DeviceId,
    /// Действие.
    pub action: Action,
    /// Время отправки (миллисекунды с момента старта узла).
    pub timestamp: u64,
    /// Оставшееся количество прыжков (TTL).
    pub ttl: u8,
}

impl Command {
    pub fn new(
        id: u32,
        device_id: DeviceId,
        action: Action,
        timestamp: u64,
        ttl: u8,
    ) -> Result<Command, CreateCommandError> {
        Ok(Command {
            id,
            device_id,
            action,
            timestamp,
            ttl,
        })
    }
}

/// Подтверждение выполнения команды.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ack {
    /// ID команды, на которую отвечаем.
    pub command_id: u32,
    /// Код результата (0 - успех, иначе код ошибки).
    pub result_code: u8,
    /// Новое состояние устройства после выполнения (опционально).
    pub new_state: Option<DeviceState>,
}

/// Запрос состояния одного или всех устройств.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    /// Если None — запросить все устройства.
    pub device_ids: Option<Vec<DeviceId>>,
    /// ID запроса для сопоставления с ответом.
    pub request_id: u32,
}

/// Ответ на запрос состояния.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// ID запроса, на который отвечаем.
    pub request_id: u32,
    /// Список пар (device_id, состояние).
    pub devices: Vec<(DeviceId, DeviceState)>,
}

/// Состояние устройства (значения полей).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceState {
    /// Включено / выключено.
    pub on: Option<bool>,
    /// Уровень (яркость, громкость).
    pub level: Option<u8>,
    /// Числовое значение (температура, влажность).
    pub value: Option<f32>,
}

pub enum CreateAnnounceError {
    InvalidTtl,
}

pub enum CreateCommandError {
    InvalidTtl,
}

pub enum TransportError {}
