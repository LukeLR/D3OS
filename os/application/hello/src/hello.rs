#![no_std]

extern crate alloc;

use concurrent::{process, thread};
#[allow(unused_imports)]
use runtime::*;
use terminal::{print, println};
use alloc::slice;

#[unsafe(no_mangle)]
pub fn main() {
    let process = process::current().unwrap();
    let thread = thread::current().unwrap();

    println!("Hello from Thread [{}] in Process [{}]!\n", thread.id(), process.id());

    println!("Arguments:");
    let args = env::args();
    for arg in args {
        println!("  {}", arg);
    }
    
    unsafe {
        let ptr = 0xb73898 as *const u8;
        let test_slice = slice::from_raw_parts(ptr, 22);
        let test_str = str::from_utf8(test_slice).unwrap();
        println!("Test str: {}, Address: 0x{:x}", test_str, test_str.as_ptr() as u64);
    }
}
