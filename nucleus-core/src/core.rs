extern crate alloc;

use alloc::{string::String, vec::Vec};
use hashbrown::HashMap;
use nucleus_mesh::{Ack, Command, FloodRouter, Message, Query, Response, Transport};
use thiserror::Error;

pub struct NucleusCore<T> {
    devices: HashMap<String, String>,
    router: FloodRouter,
    transport: T,
}

impl<T> NucleusCore<T>
where
    T: Transport,
{
    pub async fn handle_message(&mut self, msg: Message) -> Result<(), CoreError> {
        match msg {
            Message::Command(cmd) => {
                if self.devices.contains_key(&cmd.device_id) {
                    // Исполняем команду
                    self.execute_command(&cmd);
                    // Отправляем Ack обратно
                    let ack = Ack {
                        command_id: cmd.id,
                        // TODO: pass new state
                        new_state: None,
                        result_code: 0,
                    };
                    let _ = self
                        .transport
                        .send(&postcard::to_allocvec(&Message::Ack(ack)).unwrap())
                        .await;
                } else if self.router.should_forward(&cmd) {
                    // Ретранслируем
                    let forwarded = self.router.forward(cmd);
                    let _ = self
                        .transport
                        .send(&postcard::to_allocvec(&Message::Command(forwarded))?)
                        .await;
                }

                Ok(())
            }
            Message::Query(query) => {
                // Отправляем Response со всеми устройствами или фильтрованными
                let response = self.build_response(&query);
                // TODO: выбрать правильный размер, у структур тоже определить константу
                let _ = self
                    .transport
                    .send(&postcard::to_allocvec(&Message::Response(response))?)
                    .await;

                Ok(())
            }
            _ => Err(CoreError::UnknownMessage),
        }
    }

    fn build_response(&self, query: &Query) -> Response {
        Response {
            request_id: 1,
            devices: Vec::new(),
        }
    }

    fn execute_command(&self, command: &Command) {}
}

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Unknown message received")]
    UnknownMessage,

    // TODO: написать From для ошибок, т.к. thiserror требует std
    #[error("Failed to serialize message")]
    SerializationError(#[from] postcard::Error),
}
