/* ╔═════════════════════════════════════════════════════════════════════════╗
   ║ Module: lib                                                             ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Descr.: All system calls (starting with sys_).                          ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Author: Fabian Ruhland & Michael Schoettner, 30.8.2024, HHU             ║
   ╚═════════════════════════════════════════════════════════════════════════╝
*/



use alloc::string::String;
use crate::scheduler;
use signal::signal_handler::SignalHandler;
use signal::signal_vector::SignalVector;

pub fn sys_meltdown_copy_to_kernel_memory(string_content: *mut u8, string_len: usize) -> String {
    let string;
    unsafe {
        string = String::from_raw_parts(string_content, string_len, string_len);
    }
    println!("Copying {} to kernel memory", string);
    let string_clone = string.clone();
    
    println!("received string_content at {:p}, constructed old_address: {:p}, ptr: {:p}, new_address: {:p}, ptr: {:p}", string_content, &string, string.as_ptr(), &string_clone, string_clone.as_ptr());
    
    return string_clone;
}
