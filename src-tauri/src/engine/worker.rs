use crate::app_state::AppState;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone)]
pub struct RunControl {
    active: Arc<AtomicBool>,
    generation: u64,
    state: Arc<AppState>,
}

impl RunControl {
    pub fn new(state: Arc<AppState>, generation: u64, active: Arc<AtomicBool>) -> Self {
        Self {
            active,
            generation,
            state,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
            && self.state.run_gen.load(Ordering::Acquire) == self.generation
    }
}
