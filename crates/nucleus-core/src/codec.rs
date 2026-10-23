//! Кодек кадров протокола.
//!
//! Кадр: `[magic "NUC"][version u8][postcard-payload]`.
//! Версия кадра должна совпадать с [`PROTOCOL_VERSION`] — формат postcard
//! не самоописываемый, поэтому несовместимую версию молча отбрасываем
//! на уровне вызова (drop + лог).

use alloc::vec::Vec;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

/// Магический префикс кадра Nucleus.
pub const FRAME_MAGIC: [u8; 3] = *b"NUC";

/// Текущая версия протокола. Меняется при любой ломающей смене формата сообщений.
pub const PROTOCOL_VERSION: u8 = 1;

/// Длина заголовка: magic (3 байта) + version (1 байт).
pub const HEADER_LEN: usize = FRAME_MAGIC.len() + 1;

/// Ошибка кодирования/декодирования кадра.
#[derive(Debug, Clone, Error)]
pub enum FrameError {
    #[error("bad frame magic")]
    BadMagic,
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u8),
    #[error("failed to encode payload")]
    Encode(postcard::Error),
    #[error("failed to decode payload")]
    Decode(#[from] postcard::Error),
}

/// Кодирует сообщение в кадр: magic + версия + postcard-полезная нагрузка.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, FrameError> {
    let payload = postcard::to_allocvec(value).map_err(FrameError::Encode)?;
    let mut frame = Vec::with_capacity(HEADER_LEN + payload.len());
    frame.extend_from_slice(&FRAME_MAGIC);
    frame.push(PROTOCOL_VERSION);
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Декодирует кадр, требуя текущей версии протокола.
pub fn decode<T: DeserializeOwned>(frame: &[u8]) -> Result<T, FrameError> {
    decode_versioned(frame, PROTOCOL_VERSION)
}

/// Декодирует кадр, требуя указанной версии протокола.
pub fn decode_versioned<T: DeserializeOwned>(frame: &[u8], supported: u8) -> Result<T, FrameError> {
    if frame.len() < HEADER_LEN || frame[..FRAME_MAGIC.len()] != FRAME_MAGIC {
        return Err(FrameError::BadMagic);
    }
    let version = frame[FRAME_MAGIC.len()];
    if version != supported {
        return Err(FrameError::UnsupportedVersion(version));
    }
    Ok(postcard::from_bytes(&frame[HEADER_LEN..])?)
}
