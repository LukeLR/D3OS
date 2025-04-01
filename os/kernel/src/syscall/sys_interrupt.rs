/* ╔═════════════════════════════════════════════════════════════════════════╗
   ║ Module: lib                                                             ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Descr.: All system calls (starting with sys_).                          ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Author: Fabian Ruhland & Michael Schoettner, 30.8.2024, HHU             ║
   ╚═════════════════════════════════════════════════════════════════════════╝
*/

use crate::{interrupt_dispatcher};
use alloc::boxed::Box;
use interrupt::interrupt_handler::InterruptHandler;
use crate::interrupt::interrupt_dispatcher::InterruptVector;

pub fn sys_register_interrupt(index: u8, handler: Box<dyn InterruptHandler>) {
    /* TODO: To make this safe, this should only allow user-mode applications to register
     *       syscalls for themselves.
     *       That means, the interrupt dispatcher would probably need to manage interrupt
     *       overrides per thread.
     */
    interrupt_dispatcher().assign(InterruptVector::try_from(index).expect("Invalid interrupt index provided!"), handler);
}
