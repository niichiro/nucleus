//! Движок узла: nucleus-core поверх составного транспорта.
//!
//! Приём: общий INBOX (питается BLE-write'ами и ESP-NOW-приёмом).
//! Отправка: ESP-NOW broadcast (соседям) + OUTBOX → BLE notify (централам).

use alloc::boxed::Box;
use alloc::vec::Vec;
use async_trait::async_trait;
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use nucleus_core::{
    Command, DeviceExecutor, DeviceId, DeviceState, ExecuteError, FloodRouter, NucleusCore,
    Transport, TransportError,
};

/// Входящие кадры (BLE-write и ESP-NOW-приём).
pub static INBOX: Channel<CriticalSectionRawMutex, Vec<u8>, 16> = Channel::new();

/// Исходящие кадры для BLE-notify (потребитель — задача ble).
pub static OUTBOX: Channel<CriticalSectionRawMutex, Vec<u8>, 16> = Channel::new();

/// Транспорт узла: send = ESP-NOW broadcast + BLE notify, recv = INBOX.
struct NodeTransport;

#[async_trait]
impl Transport for NodeTransport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        // BLE: уведомить подключённые централы (если есть — задача ble заберёт)
        let _ = OUTBOX.try_send(data.to_vec());
        // ESP-NOW: лавинная рассылка соседям-узлам
        crate::espnow::broadcast(data);
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        Ok(INBOX.recv().await)
    }
}

/// Исполнитель без локальных устройств: любая команда «не наша» →
/// движок ретранслирует её по mesh.
#[derive(Clone)]
struct ForwardExecutor;

#[async_trait]
impl DeviceExecutor for ForwardExecutor {
    async fn execute(&mut self, command: &Command) -> Result<DeviceState, ExecuteError> {
        log::debug!("forward command for {}", command.device_id);
        Err(ExecuteError::DeviceNotFound)
    }

    async fn states(&self, _device_ids: Option<&[DeviceId]>) -> Vec<(DeviceId, DeviceState)> {
        Vec::new()
    }
}

/// Цикл движка: приём кадров из INBOX, исполнение/ретрансляция.
pub async fn run() {
    let router = FloodRouter::new(core::num::NonZeroUsize::new(64).unwrap(), 8);
    let mut core = NucleusCore::new(ForwardExecutor, router, NodeTransport);
    loop {
        if let Err(e) = core.run_once().await {
            log::warn!("engine: {e}");
        }
    }
}
