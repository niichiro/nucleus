use core::num::NonZeroUsize;
use lru::LruCache;

use crate::Command;

pub struct FloodRouter {
    seen_ids: LruCache<u32, Command>, // запоминаем последние N ID команд
    ttl: u8,
}

impl FloodRouter {
    pub fn new() -> Self {
        Self {
            seen_ids: LruCache::new(NonZeroUsize::new(2).unwrap()),
            ttl: 32,
        }
    }
    pub fn should_forward(&mut self, cmd: &Command) -> bool {
        if self.seen_ids.contains(&cmd.id) {
            return false;
        }
        self.seen_ids.put(cmd.id, cmd.clone());
        cmd.ttl > 0
    }
    pub fn forward(&mut self, mut cmd: Command) -> Command {
        cmd.ttl -= 1;
        cmd
    }
}
