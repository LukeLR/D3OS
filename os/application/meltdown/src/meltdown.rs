#![no_std]

extern crate alloc;

#[allow(unused_imports)]
use runtime::*;
use terminal::{print, println};

#[unsafe(no_mangle)]
pub fn main() {
    println!("Meltdown start\n");
}
