#![no_std]

#[allow(unused_imports)]
use runtime::*;
use terminal::{print, println};

const PAGE_SIZE: usize = 256;
#[allow(dead_code)]
pub struct MemoryPage([u128; PAGE_SIZE]);

#[unsafe(no_mangle)]
pub fn main() {
	println!("Alloctest start\n");
	
	let _mem = MemoryPage([0; PAGE_SIZE]);
}
