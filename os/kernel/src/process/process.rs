/* ╔═════════════════════════════════════════════════════════════════════════╗
   ║ Module: process                                                         ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Implementation of processes.                                            ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Author: Fabian Ruhland, HHU                                             ║
   ╚═════════════════════════════════════════════════════════════════════════╝
*/
use alloc::sync::Arc;
use alloc::vec::Vec;
use log::trace;
use x86_64::structures::paging::page::PageRange;
use x86_64::structures::paging::{Page, PageTableFlags};
use x86_64::VirtAddr;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::Relaxed;
use crate::memory::MemorySpace;
use crate::{ process_manager, scheduler};
use crate::memory::pages::Paging;
use crate::memory::vmm::VirtualAddressSpace;
use crate::signal::signal_dispatcher::SignalDispatcher;
use crate::memory::vma::VirtualMemoryArea;

static PROCESS_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn next_process_id() -> usize {
    PROCESS_ID_COUNTER.fetch_add(1, Relaxed)
}


pub struct Process {
    pub id: usize,
    pub usermode_address_space: VirtualAddressSpace,
    pub kernelmode_address_space: VirtualAddressSpace,
    pub signal_dispatcher: SignalDispatcher,
}


impl Process {
    pub fn new(usermode_address_space: VirtualAddressSpace, kernelmode_address_space: VirtualAddressSpace) -> Self {
        Self {
            id: next_process_id(),
            usermode_address_space,
            kernelmode_address_space,
            signal_dispatcher: SignalDispatcher::new(),
        }
    }

    /// Return the id of the process
    pub fn id(&self) -> usize {
        self.id
    }

    pub fn exit(&self) {
        process_manager().write().exit(self.id);
    }

    /// Return the ids of all threads of the process
    pub fn thread_ids(&self) -> Vec<usize> {
        scheduler().active_thread_ids().iter()
            .filter(|&&thread_id| {
                scheduler().thread(thread_id).is_some_and(|thread| thread.process().id() == self.id)
            }).copied().collect()
    }

    pub fn kill_all_threads_but_current(&self) {
        self.thread_ids().iter()
            .filter(|&&thread_id| thread_id != scheduler().current_thread().id())
            .for_each(|&thread_id| scheduler().kill(thread_id));
    }

    /// Grow the heap.
    /// 
    /// This is called from the page fault handler if we have a page fault in
    /// memory that is part of the heap VMA, but not yet mapped.
    pub fn grow_heap(&self, heap: &VirtualMemoryArea, fault_addr: VirtAddr) {
        let page = Page::containing_address(fault_addr);
        trace!("lazily mapping heap page {page:?} at 0x{fault_addr:x}");
        self.kernelmode_address_space.map_partial_vma(heap,
            PageRange {
                start: page,
                end: page + 1,
            },
            MemorySpace::User,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
        );
    }


    pub fn dump(&self) {
        self.usermode_address_space.dump(self.id, MemorySpace::User);
        self.kernelmode_address_space.dump(self.id, MemorySpace::Kernel);
    }

}
