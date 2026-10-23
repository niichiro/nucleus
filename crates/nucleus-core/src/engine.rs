// nucleus-core/src/engine.rs

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use async_trait::async_trait;
use thiserror::Error;

use crate::{
    codec, Ack, Command, DeviceId, DeviceState, FloodRouter, FrameError, Message, Response,
    Transport, TransportError,
};

/// Ошибка исполнения команды локальным узлом.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecuteError {
    #[error("device not found on this node")]
    DeviceNotFound,
    #[error("execution failed: {0}")]
    Failed(String),
}

/// Исполнитель локальных устройств узла.
///
/// Реализуется демоном (`nucleusd`: виртуальные устройства, Zigbee) и прошивкой
/// ESP32 (GPIO). `DeviceNotFound` означает, что устройство не этого узла —
/// движок передаст команду дальше по mesh.
#[async_trait]
pub trait DeviceExecutor {
    /// Исполняет команду над локальным устройством, возвращает новое состояние.
    async fn execute(&mut self, command: &Command) -> Result<DeviceState, ExecuteError>;

    /// Состояния локальных устройств: всех или только запрошенных.
    async fn states(&self, device_ids: Option<&[DeviceId]>) -> Vec<(DeviceId, DeviceState)>;
}

/// Ошибка движка.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unknown message")]
    UnknownMessage,

    #[error("frame error: {0}")]
    Frame(#[from] FrameError),

    #[error("serialization error: {0}")]
    Serialization(#[from] postcard::Error),

    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
}

/// Код результата в Ack: 0 — успех, 1 — ошибка исполнения.
pub const ACK_OK: u8 = 0;
pub const ACK_FAILED: u8 = 1;

/// Движок узла Nucleus: обрабатывает кадры протокола над любым транспортом.
///
/// Сетевые роли (BLE-периферия/централ, ESP-NOW) остаются за транспортом;
/// исполнение устройств — за [`DeviceExecutor`].
pub struct NucleusCore<T, E> {
    executor: E,
    router: FloodRouter,
    transport: T,
}

impl<T, E> NucleusCore<T, E>
where
    T: Transport,
    E: DeviceExecutor,
{
    pub fn new(executor: E, router: FloodRouter, transport: T) -> Self {
        Self {
            executor,
            router,
            transport,
        }
    }

    /// TTL для новых команд, создаваемых этим узлом.
    pub fn ttl(&self) -> u8 {
        self.router.ttl()
    }

    /// Отправляет исходящее сообщение (кодирует в кадр протокола).
    pub async fn send(&self, msg: &Message) -> Result<(), CoreError> {
        let frame = codec::encode(msg)?;
        self.transport.send(&frame).await?;
        Ok(())
    }

    /// Принимает один кадр из транспорта и обрабатывает его.
    ///
    /// Ошибку [`CoreError::Frame`] с [`FrameError::UnsupportedVersion`]
    /// вызывающая сторона должна логировать и продолжать (drop кадра).
    pub async fn run_once(&mut self) -> Result<(), CoreError> {
        let frame = self.transport.recv().await?;
        self.handle_frame(&frame).await
    }

    /// Обрабатывает один входящий кадр (сырые байты транспорта).
    pub async fn handle_frame(&mut self, frame: &[u8]) -> Result<(), CoreError> {
        let msg = codec::decode(frame)?;
        self.handle_message(msg).await
    }

    /// Обрабатывает одно декодированное сообщение.
    pub async fn handle_message(&mut self, msg: Message) -> Result<(), CoreError> {
        match msg {
            Message::Command(cmd) => {
                match self.executor.execute(&cmd).await {
                    Ok(new_state) => {
                        let ack = Ack {
                            command_id: cmd.id,
                            result_code: ACK_OK,
                            new_state: Some(new_state),
                        };
                        self.send(&Message::Ack(ack)).await?;
                    }
                    Err(ExecuteError::DeviceNotFound) => {
                        // Устройство не наше — ретранслируем дальше по mesh
                        if self.router.should_forward(&cmd) {
                            let forwarded = self.router.forward(cmd);
                            self.send(&Message::Command(forwarded)).await?;
                        }
                    }
                    Err(ExecuteError::Failed(_reason)) => {
                        // Детали ошибки видны только исполнителю; в протоколе — result_code
                        let ack = Ack {
                            command_id: cmd.id,
                            result_code: ACK_FAILED,
                            new_state: None,
                        };
                        self.send(&Message::Ack(ack)).await?;
                    }
                }
                Ok(())
            }
            Message::Query(query) => {
                let devices = self.executor.states(query.device_ids.as_deref()).await;
                let response = Response {
                    request_id: query.request_id,
                    devices,
                };
                self.send(&Message::Response(response)).await?;
                Ok(())
            }
            _ => Err(CoreError::UnknownMessage),
        }
    }
}
