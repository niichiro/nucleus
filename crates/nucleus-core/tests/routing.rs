//! Тесты лавинного маршрутизатора: дедупликация по ID, TTL, вытеснение LRU.

use core::num::NonZeroUsize;

use nucleus_core::{Action, Command, FloodRouter};

fn cmd(id: u32, ttl: u8) -> Command {
    Command {
        id,
        device_id: "lamp:x".into(),
        action: Action::OnOff(true),
        timestamp: 0,
        ttl,
    }
}

fn router(capacity: usize, ttl: u8) -> FloodRouter {
    FloodRouter::new(NonZeroUsize::new(capacity).unwrap(), ttl)
}

#[test]
fn forwards_first_sighting() {
    let mut r = router(16, 32);
    assert!(r.should_forward(&cmd(1, 8)));
}

#[test]
fn drops_duplicate_id() {
    let mut r = router(16, 32);
    assert!(r.should_forward(&cmd(1, 8)));
    assert!(!r.should_forward(&cmd(1, 8)));
}

#[test]
fn drops_zero_ttl_without_marking_seen() {
    let mut r = router(16, 32);
    assert!(!r.should_forward(&cmd(1, 0)));
    // Нулевой TTL не помечает ID увиденным: жизнеспособный дубль пройдёт.
    assert!(r.should_forward(&cmd(1, 8)));
}

#[test]
fn forward_decrements_ttl() {
    let mut r = router(16, 32);
    assert_eq!(r.forward(cmd(5, 3)).ttl, 2);
    assert_eq!(r.forward(cmd(5, 0)).ttl, 0);
}

#[test]
fn lru_evicts_oldest_ids() {
    let mut r = router(2, 32);
    assert!(r.should_forward(&cmd(1, 8)));
    assert!(r.should_forward(&cmd(2, 8)));
    assert!(r.should_forward(&cmd(3, 8))); // вытесняет id=1
    assert!(r.should_forward(&cmd(1, 8))); // id=1 снова «новый»
}

#[test]
fn ttl_getter_returns_configured_value() {
    assert_eq!(router(16, 7).ttl(), 7);
}
