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
    int_vectors: Vec<Mutex<Vec<Box<dyn SignalHandler>>>>,
}

unsafe impl Send for SignalDispatcher {}
unsafe impl Sync for SignalDispatcher {}

pub fn handle_signal() {
	println!("Handling signal...");
}

impl SignalDispatcher {
    pub fn new() -> Self {
        let mut int_vectors = Vec::<Mutex<Vec<Box<dyn SignalHandler>>>>::new();
        for _ in 0..MAX_VECTORS {
            int_vectors.push(Mutex::new(Vec::new()));
        }

        Self { int_vectors }
    }

    pub fn assign(&self, vector: SignalVector, handler: Box<dyn SignalHandler>) {
        match self.int_vectors.get(vector as usize) {
            Some(vec) => vec.lock().push(handler),
            None => panic!("Assigning signal handler to illegal vector number {}!", vector as u8)
        }
    }

    pub fn dispatch(&self, signal: u8) {
        let handler_vec_mutex = self.int_vectors.get(signal as usize).unwrap_or_else(|| panic!("Signal Dispatcher: No handler vec assigned for signal [{}]!", signal));
        let handler_vec = handler_vec_mutex.try_lock();
        // TODO: Do we need to force unlock here?

        if handler_vec.iter().is_empty() {
            panic!("Signal Dispatcher: No handler registered for signal [{}]!", signal);
        }

        for handler in handler_vec.unwrap().iter_mut() {
            handler.trigger();
        }
    }
}
