use signal::signal_handler::SignalHandler;
use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::Mutex;
use core::panic;
use core::convert::TryFrom;
use core::result::Result;
use core::result::Result::{Ok, Err};
use core::marker::{Sync, Send};
use core::option::Option::Some;
use signal::signal_vector::{SignalVector, MAX_VECTORS};

pub struct SignalDispatcher {
    int_vectors: Vec<Mutex<u64>>,
}

unsafe impl Send for SignalDispatcher {}
unsafe impl Sync for SignalDispatcher {}

pub fn handle_signal() {
	println!("Handling signal...");
}

impl SignalDispatcher {
    pub fn new() -> Self {
        let mut int_vectors = Vec::<Mutex<u64>>::new();
        for _ in 0..MAX_VECTORS {
            int_vectors.push(Mutex::new(0));
        }

        Self { int_vectors }
    }

    pub fn assign(&self, vector: SignalVector, handler: u64) {
        match self.int_vectors.get(vector as usize) {
            Some(vec) => *vec.lock() = handler,
            None => panic!("Assigning signal handler to illegal vector number {}!", vector as u8)
        }
    }

    /*pub fn dispatch(&self, signal: u8) {
        let handler_vec_mutex = self.int_vectors.get(signal as usize).unwrap_or_else(|| panic!("Signal Dispatcher: No handler vec assigned for signal [{}]!", signal));
        let handler_vec = handler_vec_mutex.try_lock();
        // TODO: Do we need to force unlock here?

        if handler_vec.iter().is_empty() {
            panic!("Signal Dispatcher: No handler registered for signal [{}]!", signal);
        }

        for handler in handler_vec.unwrap().iter_mut() {
            handler.trigger();
        }
    }*/
    
    pub fn get(&self, vector: SignalVector) -> Option<u64> {
        match self.int_vectors.get(vector as usize) {
            Some(vec) => Some(*vec.lock()), // Todo do we need to free the lock?
            None => None
        }
    }
}
