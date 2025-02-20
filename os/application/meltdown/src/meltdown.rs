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

pub fn maccess(pointer: *const usize) {
    unsafe {
        asm!(
            "mov {tmp}, [{x}]",
            x = in(reg) pointer,
            tmp = out(reg) _,
        );
    }
}

pub fn flush(pointer: *const usize) {
    unsafe {
        asm!(
            "clflush [{x}]",
            x = in(reg) pointer,
        );
    }
}

pub fn detect_flush_reload_threshold() -> u64{
    let mut reload_time: u64 = 0;
    let mut flush_reload_time: u64 = 0;
    let count: u64 = 10000000;
    let dummy: usize = 0; // Use single value instead of array ok?
    let pointer: *const usize;
    let mut start_time: u64;
    let mut end_time: u64;
    
    pointer = &dummy;
    
    maccess(pointer);
    for _ in 0..count {
        start_time = rdtsc();
        maccess(pointer);
        end_time = rdtsc();
        reload_time += end_time - start_time;
    }
    
    for _ in 0..count {
        start_time = rdtsc();
        maccess(pointer);
        end_time = rdtsc();
        flush(pointer);
        flush_reload_time += end_time - start_time;
    }
    
    println!("Total time: reload: {} flush+reload: {}", reload_time, flush_reload_time);
    reload_time /= count;
    flush_reload_time /= count;
    println!("Average time: reload: {} flush+reload: {}", reload_time, flush_reload_time);
    let threshold = (flush_reload_time + reload_time * 2) / 3;
    println!("Threshold: {}", threshold);
    return threshold;
}

#[unsafe(no_mangle)]
pub fn main() {
    println!("Meltdown start\n");
    
    println!("Current CPU time: {}", rdtsc());
    detect_flush_reload_threshold();
}
