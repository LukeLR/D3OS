/* ╔═════════════════════════════════════════════════════════════════════════╗
   ║ Module: lib                                                             ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Descr.: Interrupt interface in user mode.                               ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Author: Fabian Ruhland, Michael Schoettner, Lukas Rose 01.04.2025, HHU  ║
   ╚═════════════════════════════════════════════════════════════════════════╝
*/
#![no_std]

extern crate alloc;
pub mod interrupt_handler;

use interrupt_handler::InterruptHandler;
use syscall::{syscall, SystemCall};
use alloc::boxed::Box;
use terminal::{print, println};

pub fn register_interrupt(index: u8, handler: Box<dyn InterruptHandler>) {
    println!("Registering interrupt for {} at address {:p}", index, &*handler as *const dyn InterruptHandler);
    syscall(SystemCall::RegisterInterrupt, &[index as usize, &*handler as *const dyn InterruptHandler as *const () as usize]);
}
