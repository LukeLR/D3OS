/* ╔═════════════════════════════════════════════════════════════════════════╗
   ║ Module: lib                                                             ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Descr.: Interrupt interface in user mode.                               ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Author: Fabian Ruhland, Michael Schoettner, Lukas Rose 01.04.2025, HHU  ║
   ╚═════════════════════════════════════════════════════════════════════════╝
*/
#![no_std]

pub mod interrupt_handler;

use interrupt_handler::InterruptHandler;
