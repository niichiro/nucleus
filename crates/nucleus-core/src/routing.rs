// nucleus-core/src/routing.rs

use core::num::NonZeroUsize;
use lru::LruCache;

use crate::Command;

/// Лавинный маршрутизатор: дедупликация команд по ID + контроль TTL.
///
/// Емкость LRU должна быть заметно больше ожидаемого числа уникальных команд,
/// живущих в сети одновременно, иначе повторно увиденный ID примется за новый.
pub struct FloodRouter {
    seen_ids: LruCache<u32, ()>,
    ttl: u8,
}

impl FloodRouter {
    /// `capacity` — сколько последних ID команд помним;
    /// `ttl` — TTL для новых команд, создаваемых этим узлом (см. [`Self::ttl`]).
    pub fn new(capacity: NonZeroUsize, ttl: u8) -> Self {
        Self {
            seen_ids: LruCache::new(capacity),
            ttl,
        }
    }

    /// TTL по умолчанию для исходящих команд этого узла.
    pub fn ttl(&self) -> u8 {
        self.ttl
    }

    /// Нужно ли ретранслировать команду: false, если TTL исчерпан
    /// или ID уже видели. Помечает ID увиденным.
    pub fn should_forward(&mut self, cmd: &Command) -> bool {
        if cmd.ttl == 0 || self.seen_ids.contains(&cmd.id) {
            return false;
        }
        self.seen_ids.put(cmd.id, ());
        true
    }

    /// Уменьшает TTL перед передачей дальше.
    pub fn forward(&mut self, mut cmd: Command) -> Command {
        cmd.ttl = cmd.ttl.saturating_sub(1);
        cmd
    }
}
