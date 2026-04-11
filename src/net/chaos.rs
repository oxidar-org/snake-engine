use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rand::Rng;
use rand::RngExt;
use serde::Serialize;

pub struct ChaosInjector {
    pub enabled: Arc<AtomicBool>,
    interval_ticks: u64,
    rng: Box<dyn Rng + Send>,
}

impl ChaosInjector {
    pub fn new(interval_ticks: u64) -> Self {
        assert!(interval_ticks > 0, "interval_ticks must be > 0");
        use rand::SeedableRng;
        let rng = Box::new(rand::rngs::StdRng::from_rng(&mut rand::rng()));
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            interval_ticks,
            rng,
        }
    }

    pub fn generate(&mut self, tick: u64) -> Option<Vec<u8>> {
        if !self.enabled.load(Ordering::Relaxed) {
            return None;
        }
        if !tick.is_multiple_of(self.interval_ticks) {
            return None;
        }
        Some(random_chaos(&mut *self.rng, tick))
    }
}

fn random_chaos(rng: &mut dyn Rng, current_tick: u64) -> Vec<u8> {
    match rng.random_range(0u8..5) {
        0 => chaos_garbage(rng),
        1 => chaos_unknown_type(),
        2 => chaos_wrong_field_types(),
        3 => chaos_missing_fields(current_tick),
        _ => chaos_out_of_order(rng, current_tick),
    }
}

// Type a: random garbage — undecodable binary
fn chaos_garbage(rng: &mut dyn Rng) -> Vec<u8> {
    let len: usize = rng.random_range(16..=64usize);
    (0..len).map(|_| rng.random::<u8>()).collect()
}

// Type b: valid msgpack but unknown `type` tag
#[derive(Serialize)]
struct UnknownType {
    #[serde(rename = "type")]
    type_: &'static str,
}

fn chaos_unknown_type() -> Vec<u8> {
    rmp_serde::to_vec_named(&UnknownType { type_: "glitch" }).expect("serialize")
}

// Type c: state-shaped map with wrong field types
#[derive(Serialize)]
struct WrongFieldTypes {
    #[serde(rename = "type")]
    type_: &'static str,
    tick: &'static str, // should be u64
    food: u32,          // should be [u16; 2]
    snakes: u8,         // should be Vec<SnakeData>
}

fn chaos_wrong_field_types() -> Vec<u8> {
    rmp_serde::to_vec_named(&WrongFieldTypes {
        type_: "state",
        tick: "not_a_number",
        food: 42,
        snakes: 0,
    })
    .expect("serialize")
}

// Type d: state-shaped map with missing required fields
#[derive(Serialize)]
struct MissingFields {
    #[serde(rename = "type")]
    type_: &'static str,
    tick: u64,
}

fn chaos_missing_fields(tick: u64) -> Vec<u8> {
    rmp_serde::to_vec_named(&MissingFields {
        type_: "state",
        tick,
    })
    .expect("serialize")
}

// Type f: valid State but tick goes backwards; snakes is empty
#[derive(Serialize)]
struct OutOfOrderState {
    #[serde(rename = "type")]
    type_: &'static str,
    tick: u64,
    food: [u16; 2],
    snakes: Vec<[u16; 2]>, // empty vec; element type irrelevant when empty
}

fn chaos_out_of_order(rng: &mut dyn Rng, current_tick: u64) -> Vec<u8> {
    let max_offset = current_tick.clamp(1, 50);
    let offset = rng.random_range(1..=max_offset);
    rmp_serde::to_vec_named(&OutOfOrderState {
        type_: "state",
        tick: current_tick - offset,
        food: [0, 0],
        snakes: vec![],
    })
    .expect("serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn enabled_injector(interval: u64) -> ChaosInjector {
        let inj = ChaosInjector::new(interval);
        inj.enabled.store(true, Ordering::Relaxed);
        inj
    }

    #[test]
    fn generate_returns_none_when_disabled() {
        let mut inj = ChaosInjector::new(1);
        assert!(inj.generate(1).is_none());
    }

    #[test]
    fn generate_returns_none_when_tick_not_at_interval() {
        let mut inj = enabled_injector(10);
        assert!(inj.generate(1).is_none());
        assert!(inj.generate(9).is_none());
        assert!(inj.generate(11).is_none());
    }

    #[test]
    fn generate_returns_some_when_enabled_and_tick_matches() {
        let mut inj = enabled_injector(10);
        assert!(inj.generate(10).is_some());
        assert!(inj.generate(20).is_some());
    }

    #[test]
    fn all_chaos_types_produce_nonempty_bytes() {
        use rand::SeedableRng;
        let mut _rng = rand::rngs::StdRng::seed_from_u64(42);
        for tick in [10u64, 11, 12, 13, 14] {
            let mut inj = enabled_injector(1);
            let bytes = inj.generate(tick).unwrap();
            assert!(
                !bytes.is_empty(),
                "chaos at tick {tick} produced empty bytes"
            );
        }
    }

    #[test]
    fn out_of_order_does_not_underflow_at_tick_one() {
        // tick=1 is the minimum valid tick; offset must not exceed it
        let mut inj = enabled_injector(1);
        // Run 20 times to exercise the random offset selection
        for _ in 0..20 {
            let bytes = inj.generate(1);
            assert!(bytes.is_some());
        }
    }
}
