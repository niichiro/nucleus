extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use async_trait::async_trait;

use crate::TransportError;

#[async_trait]
pub trait Transport {
    async fn send(&self, data: &[u8]) -> Result<(), TransportError>;
    async fn recv(&mut self) -> Result<Vec<u8>, TransportError>;
}
