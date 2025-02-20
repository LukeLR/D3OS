#![no_std]

extern crate alloc;

#[allow(unused_imports)]
use runtime::*;
use terminal::{print, println};

use core::arch::asm;

pub fn rdtsc() -> u64 {
    let high: u32;
    let low: u32;
    unsafe {
        asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high
        );
    }
    ((high as u64) << 32) | (low as u64)
}

#[unsafe(no_mangle)]
pub fn main() {
    println!("Meltdown start\n");
    
    println!("cpu cycles: {}", rdtsc());
    println!("cpu cycles: {}", rdtsc());
}
