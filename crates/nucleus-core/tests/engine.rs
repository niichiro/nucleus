//! Интеграционные тесты движка: два узла через in-memory транспорт.

use core::num::NonZeroUsize;
use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use nucleus_core::{
    codec, Action, Command, DeviceExecutor, DeviceState, ExecuteError, FloodRouter, Message,
    NucleusCore, Query, Transport, TransportError, ACK_FAILED, ACK_OK,
};
use tokio::sync::mpsc;

/// Транспорт поверх пары каналов: send уходит партнёру, recv читает от партнёра.
struct Pipe {
    outgoing: mpsc::UnboundedSender<Vec<u8>>,
    incoming: mpsc::UnboundedReceiver<Vec<u8>>,
}

#[async_trait]
impl Transport for Pipe {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError> {
        self.outgoing
            .send(data.to_vec())
            .map_err(|_| TransportError::Send)
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        self.incoming.recv().await.ok_or(TransportError::Closed)
    }
}

/// Пара соединённых транспортов (a → b, b → a).
fn pipe_pair() -> (Pipe, Pipe) {
    let (a2b_tx, a2b_rx) = mpsc::unbounded_channel();
    let (b2a_tx, b2a_rx) = mpsc::unbounded_channel();
    (
        Pipe {
            outgoing: a2b_tx,
            incoming: b2a_rx,
        },
        Pipe {
            outgoing: b2a_tx,
            incoming: a2b_rx,
        },
    )
}

fn router() -> FloodRouter {
    FloodRouter::new(NonZeroUsize::new(64).unwrap(), 8)
}

fn command(id: u32, device: &str) -> Message {
    Message::Command(Command {
        id,
        device_id: device.into(),
        action: Action::OnOff(true),
        timestamp: 1,
        ttl: 8,
    })
}

/// Мок исполнителя: устройства в HashMap, устройство с ключом "err:*" падает.
#[derive(Default)]
struct MockExecutor {
    devices: HashMap<String, DeviceState>,
}

#[async_trait]
impl DeviceExecutor for MockExecutor {
    async fn execute(&mut self, command: &Command) -> Result<DeviceState, ExecuteError> {
        if command.device_id.starts_with("err:") {
            return Err(ExecuteError::Failed("driver exploded".into()));
        }
        match self.devices.get_mut(&command.device_id) {
            None => Err(ExecuteError::DeviceNotFound),
            Some(state) => {
                match command.action {
                    Action::OnOff(v) => state.on = Some(v),
                    Action::SetLevel(l) => state.level = Some(l),
                    Action::SetValue(x) => state.value = Some(x),
                }
                Ok(state.clone())
            }
        }
    }

    async fn states(&self, device_ids: Option<&[String]>) -> Vec<(String, DeviceState)> {
        match device_ids {
            None => self
                .devices
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            Some(ids) => ids
                .iter()
                .filter_map(|id| self.devices.get(id).map(|s| (id.clone(), s.clone())))
                .collect(),
        }
    }
}

/// Читает следующий кадр из транспорта с таймаутом.
async fn next_frame(pipe: &mut Pipe) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(1), pipe.recv())
        .await
        .expect("timeout waiting for frame")
        .expect("transport closed")
}

#[tokio::test]
async fn local_command_gets_ack_with_new_state() {
    let (mut pipe_a, pipe_b) = pipe_pair();
    let mut executor = MockExecutor::default();
    executor.devices.insert(
        "lamp:kitchen".into(),
        DeviceState {
            on: Some(false),
            level: None,
            value: None,
        },
    );
    let mut b = NucleusCore::new(executor, router(), pipe_b);

    // Узел A (без движка) отправляет команду устройству узла B.
    pipe_a
        .send(&codec::encode(&command(7, "lamp:kitchen")).unwrap())
        .await
        .unwrap();

    b.run_once().await.unwrap();

    let ack: Message = codec::decode(&next_frame(&mut pipe_a).await).unwrap();
    match ack {
        Message::Ack(ack) => {
            assert_eq!(ack.command_id, 7);
            assert_eq!(ack.result_code, ACK_OK);
            assert_eq!(
                ack.new_state,
                Some(DeviceState {
                    on: Some(true),
                    level: None,
                    value: None
                })
            );
        }
        other => panic!("expected Ack, got {other:?}"),
    }
}

#[tokio::test]
async fn failed_command_gets_failed_ack() {
    let (mut pipe_a, pipe_b) = pipe_pair();
    let mut b = NucleusCore::new(MockExecutor::default(), router(), pipe_b);

    pipe_a
        .send(&codec::encode(&command(8, "err:boom")).unwrap())
        .await
        .unwrap();
    b.run_once().await.unwrap();

    let ack: Message = codec::decode(&next_frame(&mut pipe_a).await).unwrap();
    match ack {
        Message::Ack(ack) => {
            assert_eq!(ack.command_id, 8);
            assert_eq!(ack.result_code, ACK_FAILED);
            assert_eq!(ack.new_state, None);
        }
        other => panic!("expected Ack, got {other:?}"),
    }
}

#[tokio::test]
async fn forwards_remote_command_and_dedups() {
    let (mut pipe_a, pipe_b) = pipe_pair();
    // Исполнитель без устройств — команда чужая, должна уйти дальше по mesh.
    let mut b = NucleusCore::new(MockExecutor::default(), router(), pipe_b);

    pipe_a
        .send(&codec::encode(&command(9, "lamp:elsewhere")).unwrap())
        .await
        .unwrap();
    b.run_once().await.unwrap();

    // B ретранслирует с уменьшенным TTL.
    let forwarded: Message = codec::decode(&next_frame(&mut pipe_a).await).unwrap();
    match forwarded {
        Message::Command(c) => {
            assert_eq!(c.id, 9);
            assert_eq!(c.ttl, 7);
        }
        other => panic!("expected Command, got {other:?}"),
    }

    // Дубль той же команды B больше не ретранслирует.
    pipe_a
        .send(&codec::encode(&command(9, "lamp:elsewhere")).unwrap())
        .await
        .unwrap();
    b.run_once().await.unwrap();
    assert!(pipe_a.incoming.try_recv().is_err());
}

#[tokio::test]
async fn query_returns_states_and_filters_by_ids() {
    let (mut pipe_a, pipe_b) = pipe_pair();
    let mut executor = MockExecutor::default();
    executor.devices.insert(
        "lamp:living".into(),
        DeviceState {
            on: Some(true),
            level: Some(50),
            value: None,
        },
    );
    executor.devices.insert(
        "lamp:kitchen".into(),
        DeviceState {
            on: Some(false),
            level: Some(100),
            value: None,
        },
    );
    let mut b = NucleusCore::new(executor, router(), pipe_b);

    let query = Message::Query(Query {
        device_ids: Some(vec!["lamp:kitchen".into(), "lamp:missing".into()]),
        request_id: 55,
    });
    pipe_a.send(&codec::encode(&query).unwrap()).await.unwrap();
    b.run_once().await.unwrap();

    let response: Message = codec::decode(&next_frame(&mut pipe_a).await).unwrap();
    match response {
        Message::Response(r) => {
            assert_eq!(r.request_id, 55);
            // Только реально существующее из запрошенных
            assert_eq!(r.devices.len(), 1);
            assert_eq!(r.devices[0].0, "lamp:kitchen");
            assert_eq!(r.devices[0].1.level, Some(100));
        }
        other => panic!("expected Response, got {other:?}"),
    }
}
