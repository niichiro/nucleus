// nucleus-core/src/transport.rs

use alloc::boxed::Box;
use alloc::vec::Vec;
use async_trait::async_trait;
use thiserror::Error;

/// Ошибка транспорта (BLE, ESP-NOW, in-memory и т.д.).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("send failed")]
    Send,
    #[error("transport closed")]
    Closed,
}

/// Двунаправленный транспорт кадр-уровня: отправка и приём сырых байтов.
// clippy (1.99+): async_trait генерирует #[must_use] на возвращаемых boxed future
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Transport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError>;
    async fn recv(&mut self) -> Result<Vec<u8>, TransportError>;
}
