//! Ядро Nucleus: протокол и движок узла (no_std).
//!
//! Общее ядро для всех типов узлов (Orange Pi, ESP32, телефон):
//! типы сообщений (`messages`), бинарный кодек с кадрированием и версионированием
//! (`codec`), лавинная маршрутизация (`routing`), абстракция транспорта
//! (`transport`) и движок обработки сообщений (`engine`).
#![no_std]

extern crate alloc;

pub mod codec;
pub mod engine;
pub mod messages;
pub mod routing;
pub mod transport;

pub use codec::*;
pub use engine::*;
pub use messages::*;
pub use routing::*;
pub use transport::*;
