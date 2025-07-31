/* ╔═════════════════════════════════════════════════════════════════════════╗
   ║ Module: virtual memory management                                       ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Functions related to a virtual memory management of a process address   ║
   ║ space. This includes managing virtual memory areas, allocating frames   ║
   ║ for full or partial vmas, as well as creating page mappings.            ║
   ║                                                                         ║
   ║ Public convenience functions:                                           ║
   ║   - kernel_map_devm_identity  map physical device memory in kernel space║
   ║                               (identity mapped) and allocate a vma      ║
   ║   - kernel_alloc_map_identity allocate page frames in kernel space and  ║
   ║                               a vma and create a identity mapping       ║
   ║   - user_alloc_map_full       create vma for pages, allocate and map it ║
   ║                               in user space.                            ║
   ║                                                                         ║
   ║ Public functions:                                                       ║
   ║   - alloc_vma                 allocate a page range in an address space ║
   ║   - alloc_pfr_for_vma         allocate pf range for full vma            ║
   ║   - alloc_pfr_for_partial_vma alloc pf range for a subrange of a vma    ║
   ║   - map_pfr_for_vma           map pf range for full vma                 ║
   ║   - map_pfr_for_partial_vma   map pf range for subrange of a vma        ║
   ║   - map_partial_vma           map a sub page range of a vma by          ║
   ║                               allocating frames as needed               ║
   ║                                                                         ║
   ║   - clone_address_space       used for process creation                 ║
   ║   - create_kernel_address_space   used for process creation             ║
   ║   - iter_vmas                 Iterate over all VMAs                     ║
   ║   - dump                      dump all VMAs of an address space         ║
   ║   - page_table_address        get root page table address               ║
   ║   - set_flags                 set page table flags                      ║
   ╟─────────────────────────────────────────────────────────────────────────╢
   ║ Author: Fabian Ruhland and Michael Schoettner                           ║
   ║         Univ. Duesseldorf, 26.05.2025                                   ║
   ╚═════════════════════════════════════════════════════════════════════════╝
*/

///
/// This module provides functions to manage virtual memory areas (VMAs) in
/// a process address space. Below is a description of steps for typical
/// memory allocations.
///
///  Map device memory:
///     => map_devmem_identity
///  
/// User stack:
///     1. alloc_vma
///     2. alloc_pfr_for_partial_vma
///     3. map_partial_vma
///
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Deref;
use core::ops::Range;
use log::{warn, info, debug, trace};
use spin::RwLock;

use x86_64::PhysAddr;
use x86_64::VirtAddr;
use x86_64::structures::paging::frame::PhysFrameRange;
use x86_64::structures::paging::page::PageRange;
use x86_64::structures::paging::{Page, PageTableFlags};

use crate::cpu;
use crate::memory::frames;
use crate::memory::frames::phys_limit;
use crate::memory::pages;
use crate::memory::pages::Paging;
use crate::memory::{MemorySpace, PAGE_SIZE};
use crate::process_manager;
use crate::boot::visible_from_usermode_region;
use crate::memory::vma::{VirtualMemoryArea, VmaType};
use crate::consts::VISIBLE_FROM_USERMODE_VIRT_START;

/// Clone address space. Used during process creation.
pub fn clone_address_space(other: &VirtualAddressSpace) -> VirtualAddressSpace {
    VirtualAddressSpace::clone(other)
}

/// Create user address space
pub fn create_user_address_space() -> VirtualAddressSpace {
    debug!("Creating new user address space");
    let page_tables = Paging::new(4);
    
    let address_space = VirtualAddressSpace::new(Arc::new(page_tables));
    let usermode_region = visible_from_usermode_region();
    let num_pages = (usermode_region.end.start_address() -
                    usermode_region.start.start_address()) /
                    usermode_region.start.size();
    
    let start_addr = VirtAddr::new(VISIBLE_FROM_USERMODE_VIRT_START as u64);
    let start_page = Page::from_start_address(start_addr).expect("Virtual start address for visible_from_userspace section not page aligned!");
    
    debug!("Alloc'ing pages from user-visible kernel code memory 0x{:x} - 0x{:x} ({} pages) at 0x{:x}",
           usermode_region.start.start_address(),
           usermode_region.end.start_address(),
           num_pages,
           start_page.start_address());
    
    let vma = address_space.alloc_vma(Some(start_page),
                                      num_pages,
                                      MemorySpace::UserAccessible,
                                      VmaType::Code,
                                      "krn_usr").expect("Couldn't allocate VirtualMemoryArea for visible_from_userspace section");
    
    debug!("Mapping user-visible kernel code into VMA");
    
    address_space.map_pfr_for_vma(&vma,
                                  usermode_region,
                                  PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE).expect("Couldn't map visible_from_userspace section into VMA!");
    
    debug!("Done creating user address space");
    address_space
}

/// Create kernel address space. Used during process creation.
pub fn create_kernel_address_space() -> VirtualAddressSpace {
    let address_space = create_user_address_space(); // Kernel address space should contain user address space as well
    // map all physical addresses 1:1
    let max_phys_addr = phys_limit().start_address();
    let range = PageRange {
        start: Page::containing_address(VirtAddr::zero()),
        end: Page::containing_address(VirtAddr::new(max_phys_addr.as_u64())),
    };

    address_space.page_tables.map(range, MemorySpace::Kernel, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    address_space
}

/// Return the last useable virtual address in canonical form
fn last_usable_virtual_address() -> u64 {
    let virtual_bits = cpu().linear_address_bits();
    (1u64 << (virtual_bits - 1)) - 1
}

/// Wrapper function
/// Allocate `frame_count` contiguous page frames.
pub fn alloc_frames(frame_count: usize) -> PhysFrameRange {
    frames::alloc(frame_count)
}

/// Wrapper function
/// Free a contiguous range of page `frames`.
pub fn free_frames(frames: PhysFrameRange) {
    unsafe {
        frames::free(frames);
    }
}

/// Wrapper function
pub fn frame_allocator_locked() -> bool {
    frames::allocator_locked()
}

pub struct VmaIterator {
    vmas: Vec<Arc<VirtualMemoryArea>>,
    index: usize,
}

impl VmaIterator {
    pub fn new(vmas: Vec<Arc<VirtualMemoryArea>>) -> Self {
        Self { vmas, index: 0 }
    }
}

impl Iterator for VmaIterator {
    type Item = Arc<VirtualMemoryArea>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.vmas.len() {
            let vma = Arc::clone(&self.vmas[self.index]);
            self.index += 1;
            Some(vma)
        } else {
            None
        }
    }
}

/// All data related to a virtual address space of a process.
#[derive(Clone)]
pub struct VirtualAddressSpace {
    virtual_memory_areas: Arc<RwLock<Vec<Arc<VirtualMemoryArea>>>>,
    page_tables: Arc<Paging>,
    first_usable_user_addr: VirtAddr, // fixed, the first usable user address
    last_usable_user_addr: VirtAddr, // fixed , the last usable user address
}

impl VirtualAddressSpace {
    /// Initialize a new virtual address space with the given `page_tables`.
    pub fn new(page_tables: Arc<Paging>) -> Self {
        Self::new_with_vmas(page_tables, Arc::new(RwLock::new(Vec::new())))
    }

    pub fn new_with_vmas(page_tables: Arc<Paging>, virtual_memory_areas: Arc<RwLock<Vec<Arc<VirtualMemoryArea>>>>) -> Self {
        let first_usable_user_addr = VirtAddr::new(crate::consts::USER_SPACE_START as u64);
        let last_usable_user_addr: VirtAddr = VirtAddr::new(last_usable_virtual_address());
        info!(
            "VirtualAddressSpace: first usable user address: 0x{:x}, last usable user address: 0x{:x}",
            first_usable_user_addr.as_u64(),
            last_usable_user_addr.as_u64()
        );
        
        Self {
            page_tables,
            virtual_memory_areas,
            first_usable_user_addr,
            last_usable_user_addr,
        }
    }

    pub fn page_tables(&self) -> Arc<Paging> {
        Arc::clone(&self.page_tables)
    }
    
    pub fn virtual_memory_areas(&self) -> Arc<RwLock<Vec<Arc<VirtualMemoryArea>>>> {
        Arc::clone(&self.virtual_memory_areas)
    }

	#[unsafe(link_section = ".visible_from_usermode")]
    pub fn load_address_space(&self) {
        self.page_tables.load();
    }
    
    pub fn load_address_space_kernel(&self) {
        self.page_tables.load_kernel();
    }
    
    pub fn address_space_loaded(&self) -> bool {
        self.page_tables.is_loaded()
    }

    /// Tries to allocate a virtual memory region for `num_pages` pages for the given `space`, `typ`, and `tag` in the address space `self`. \
    /// If `start_page` is `Some` the allocator tries to allocate the vma from the given page otherwise it will allocate from any free page. \
    /// No frames are allocated and no mappings are created in the page tables. \
    /// Returns the new [`VirtualMemoryArea`] if successful, otherwise `None`.
    pub fn alloc_vma(
        &self, start_page: Option<Page>, num_pages: u64, vma_space: MemorySpace, vma_type: VmaType, vma_tag: &str,
    ) -> Option<Arc<VirtualMemoryArea>> {
        trace!("Called alloc_vma with start_page: {:?}, num_pages: {}, vma_space: {:?}, vma_type: {:?}, vma_tag: {}", start_page, num_pages, vma_space, vma_type, vma_tag);
        
        let result = match start_page {
            Some(start_page) => {
                trace!("alloc_vma has start_page, calling alloc_at!");
                self.alloc_at(start_page, num_pages, vma_space, vma_type, vma_tag)
            },
            None => {
                trace!("alloc_vma has no start_page, calling alloc_at!");
                self.alloc(num_pages, vma_space, vma_type, vma_tag)
            },
        };
        trace!("Alloc'd a VMA: {:?}", result);
        result
    }

    /// Tries to allocate a frame range for the full `vma`. \
    /// Returns the allocated [`PhysFrameRange`] if successful, otherwise `None`.
    pub fn alloc_pf_for_vma(&self, vma: &VirtualMemoryArea) -> Option<PhysFrameRange> {
        Some(frames::alloc(vma.range.len() as usize))
    }

    /// Tries to allocate a frame range for the given `page_range` which must be within the given `vma`. \
    /// Returns the allocated [`PhysFrameRange`] if successful, otherwise `None`.
    pub fn alloc_pfr_for_partial_vma(&self, vma: &VirtualMemoryArea, page_range: PageRange) -> Option<PhysFrameRange> {
        if page_range.start < vma.range.start || page_range.end > vma.range.end {
            return None;
        }
        Some(frames::alloc(page_range.len() as usize))
    }

    /// Map `frame_range` for the full page range of the given `vma`. \
    /// The mapping will use the given `flags` for the page table entries.
    pub fn map_pfr_for_vma(&self, vma: &VirtualMemoryArea, frame_range: PhysFrameRange, mut flags: PageTableFlags) -> Result<(), i64> {
        self.map_pfr_for_partial_vma(vma, frame_range, vma.range, flags)
    }

    /// Map `frame_range` for the given page range which must be witin the given `vma`. \
    /// The mapping will use the given already allocated frames and the `flags` for the page table entries.
    pub fn map_pfr_for_partial_vma(
        &self, vma: &VirtualMemoryArea, frame_range: PhysFrameRange, page_range: PageRange, mut flags: PageTableFlags,
    ) -> Result<(), i64> {
        // Check if the number of frames of the `frame_range` is identical with the number of pages of `page_range`
        let num_frames = frame_range.end - frame_range.start;
        let num_pages = page_range.end - page_range.start;
        if num_frames != num_pages {
            warn!("Can't map {} frames into VMA with {} pages!", num_frames, num_pages);
            return Err(-1);
        }

        // Check if the flags are consistent with the vma
        flags = vma.check_and_enforce_consistency(flags);
        flags |= PageTableFlags::PRESENT;

        // Check if `page_range` is within the VMA range
        if page_range.start < vma.range.start || page_range.end > vma.range.end {
            warn!("Can't map pages {:?} - {:?} into vma {:?} - {:?}!", page_range.start, page_range.end, vma.range.start, vma.range.end);
            return Err(-1);
        }

        // Do the mapping
        self.page_tables.map_physical(frame_range, page_range, vma.space, flags);

        Ok(())
    }

    /// Allocates a virtual memory region for `num_pages` pages, starting from `first_page` \
    /// for the given `space`, `typ`, and `tag` in the address space `self`. \
    /// No mappings are created in the page tables. \
    /// Returns the new [`VirtualMemoryArea`] if successful, otherwise `None`.
    fn alloc_at(&self, first_page: Page, num_pages: u64, vma_space: MemorySpace, vma_type: VmaType, vma_tag_str: &str) -> Option<Arc<VirtualMemoryArea>> {
        let start_addr = first_page.start_address();

        let end_page = first_page + num_pages;
        let end_addr = end_page.start_address(); // still safe, since end is exclusive
        
        trace!("alloc_at: Checking bounds");

        // Bounds check against usable address range
        match vma_space {
            MemorySpace::User => {
                if start_addr < self.first_usable_user_addr || end_addr > self.last_usable_user_addr {
                    warn!("Trying to alloc_at in user memory space with invalid bounds: 0x{:x} - 0x{:x}", start_addr, end_addr);
                    return None;
                }
            }
            MemorySpace::Kernel => {
                if end_addr > self.last_usable_user_addr {
                    warn!("Trying to alloc_at in kernel memory space with invalid end address: 0x{:x}", end_addr);
                    return None;
                }
            }
            MemorySpace::UserAccessible => {
                if start_addr.as_u64() < VISIBLE_FROM_USERMODE_VIRT_START as u64 || end_addr > self.first_usable_user_addr {
                    warn!("Trying to alloc_at in UserAccessible memory space with invalid bounds: 0x{:x} - 0x{:x}", start_addr, end_addr);
                    return None;
                }
            }
        }
        
        trace!("alloc_at: Creating new VMA");
        
        // Create new VMA
        let vma_range = PageRange {
            start: first_page,
            end: first_page + num_pages,
        };
        let new_vma = Arc::new(VirtualMemoryArea::new_with_tag(vma_space, vma_range, vma_type, vma_tag_str));
        
        trace!("alloc_at: Checking for overlap with existing VMAs");
        
        // Check for overlap with existing VMAs
        let mut vmas = self.virtual_memory_areas.write();
        vmas.sort_by(|a, b| a.range.start.cmp(&b.range.start));
        trace!("alloc_at: Existing VMAs sorted {:?}", vmas);
        
        for vma in vmas.iter() {
            // Check for overlap with existing VMAs
            if vma.overlaps_with(&new_vma) {
                warn!("alloc_at: Could not allocate VMA {:?}, it overlaps with {:?}!", new_vma, vma);
                return None;
            }
        }
        
        let vma_clone = Arc::clone(&new_vma);
        trace!("alloc_at: Pushing VMA clone at {:p}", vma_clone);

        // No overlap, add new VMA
        vmas.push(vma_clone);
        trace!("alloc_at: Created a new VMA object at {:p}: {:?}", &new_vma, new_vma);
        let result = Some(new_vma);
        trace!("alloc_at: Wrapped new VMA object into optional at {:p}", &result);
        
        result
    }

    /// Allocates a virtual memory region for `num_pages` pages (starting from any free page) \
    /// for the given `space`, `typ` and `tag` in the address space `self`. \
    /// No mappings are created in the page tables. \
    /// Returns the new [`VirtualMemoryArea`] if successful, otherwise `None`.
    fn alloc(&self, num_pages: u64, vma_space: MemorySpace, vma_type: VmaType, vma_tag: &str) -> Option<Arc<VirtualMemoryArea>> {

        info!("*** VMA alloc");

        let mut vmas = self.virtual_memory_areas.write();
        vmas.sort_by(|a, b| a.range.start.cmp(&b.range.start));

        let requested_region_size = num_pages * PAGE_SIZE as u64;

        // Start searching from first usable user address
        let mut current_addr = self.first_usable_user_addr;
        for vma in vmas.iter() {
            let gap_start = current_addr;
            let gap_end = vma.range.start.start_address();

            if gap_end > gap_start {
                let gap_size = gap_end.as_u64() - gap_start.as_u64();

                if gap_size >= requested_region_size {
                    let candidate_page = Page::containing_address(gap_start);
                    drop(vmas); // release lock before recursive call
                    return self.alloc_at(candidate_page, num_pages, vma_space, vma_type, vma_tag);
                }
            }

            // Move to end of current VMA
            current_addr = vma.range.end.start_address();
        }

        // Try allocating after last VMA
        let last_addr = current_addr;
        let available = self.last_usable_user_addr.as_u64().saturating_sub(last_addr.as_u64());

        if available >= requested_region_size {
            let candidate_page = Page::containing_address(last_addr);
            trace!("Will alloc a VMA at {:?}, num_pages: {}, vma_space: {:?}, vma_type: {:?}, vma_tag: {}",
                   candidate_page, num_pages, vma_space, vma_type, vma_tag);
            return self.alloc_at(candidate_page, num_pages, vma_space, vma_type, vma_tag);
        }
        
        warn!("No space found in alloc! num_pages: {}, vma_space: {:?}, vma_type: {:?}, vma_tag: {}",
              num_pages, vma_space, vma_type, vma_tag);
        None // No space found
    }

    /// Iterate over all virtual memory areas in this address space.
    pub fn iter_vmas(&self) -> VmaIterator {
        let vmas = self.virtual_memory_areas.read().clone();
        VmaIterator::new(vmas)
    }

    /// Map the sub `page_range` of the given `vma` by allocating frames as needed.
    pub fn map_partial_vma(&self, vma: &VirtualMemoryArea, page_range: PageRange, space: MemorySpace, flags: PageTableFlags) {
        let areas = self.virtual_memory_areas.read();
        areas.iter().find(|area| (**area).deref() == vma).expect("tried to map a non-existent VMA!");
        assert!(page_range.start.start_address() >= vma.start());
        assert!(page_range.end.start_address() <= vma.end());
        self.page_tables.map(page_range, space, flags);
    }

    /// Set page table `flags` for the give page range `pages`  
    pub fn set_flags(&self, pages: PageRange, flags: PageTableFlags) {
        self.page_tables.set_flags(pages, flags);
    }

    /// Get physical address of root page table
    pub fn page_table_address(&self) -> PhysAddr {
        self.page_tables.page_table_address()
    }

    /// Dump all virtual memory areas of this address space
    pub fn dump(&self, pid: usize) {
        info!("VMAs of process [{pid}]");
        let areas = self.virtual_memory_areas.read();
        for area in areas.iter() {
            info!("{area:?}");
        }
    }

    /// Helper function to align an address up to the next page boundary.
    fn align_up(addr: u64) -> u64 {
        let ps = PAGE_SIZE as u64;
        (addr + ps - 1) & !(ps - 1)
    }

    /// Map the given page-frame range [`start_phys_addr`, `end_phys_addr`) - identity mapped in kernel space. No page frames are allocated! \
    /// `start_phys_addr` must be page aligned. \
    /// `end_phys_addr` must be greater than `start_phys_addr` but no need to be page aligned. If it is not page aligned, it will be aligned up. \
    /// A vma ist created using the parameters `typ` and `tag`.
    pub fn kernel_map_devm_identity(&self, start_phys_addr: u64, end_phys_addr: u64, flags: PageTableFlags, typ: VmaType, tag: &str) -> Page {
        assert!(end_phys_addr > start_phys_addr, "'end_phys_addr' must be larger than 'start_phys_addr'");

        // Calc page frame range (nneded for mapping))
        let start_page_frame = frames::frame_from_u64(start_phys_addr).expect("start_phys_addr is not page aligned");
        let end_page_frame = frames::frame_from_u64(Self::align_up(end_phys_addr)).expect("end_phys_addr is not page aligned");
        let pfr = PhysFrameRange {
            start: start_page_frame,
            end: end_page_frame,
        };

        // Calc page range and alloc vma
        let start_page_addr = pages::page_from_u64(start_phys_addr).expect("start_phys_addr is not page aligned");
        let end_page_addr = pages::page_from_u64(Self::align_up(end_phys_addr)).expect("end_phys_addr is not page aligned");
        let pr = PageRange {
            start: start_page_addr,
            end: end_page_addr,
        };
        let vma = self
            .alloc_vma(Some(start_page_addr), pr.len() as u64, MemorySpace::Kernel, typ, tag)
            .expect("alloc_vma failed");

        // Now we do the mapping
        self.map_pfr_for_vma(&vma, pfr, flags).expect("map_pfr_for_vma failed in map_devmem_identity");

        pr.start
    }

    /// Alloc `num_pf` page frames, en bloc, identity mapped in kernel space.
    /// A vma ist created using the parameters `typ` and `tag`.
    pub fn kernel_alloc_map_identity(&self, num_pf: u64, flags: PageTableFlags, typ: VmaType, tag: &str) -> PageRange {
        // Alloc page frame range
        let pfr = frames::alloc(num_pf as usize);

        // Create page from pfr.start
        let start_page = pages::page_from_u64(pfr.start.start_address().as_u64()).expect("pfr.start is not page aligned");

        let vma = self
            .alloc_vma(Some(start_page), pfr.len() as u64, MemorySpace::Kernel, typ, tag)
            .expect("alloc_vma failed");

        // Now we do the mapping
        self.map_pfr_for_vma(&vma, pfr, flags).expect("map_pfr_for_vma failed");

        PageRange {
            start: start_page,
            end: start_page + num_pf,
        }
    }

    /// Tries to allocate a virtual memory region for `num_pages` pages for `MemorySpace::User`, `typ`, and `tag` in the address space `self`. \
    /// If `start_page` is `Some` the allocator tries to allocate the vma from the given page otherwise it will allocate from any free page. \
    /// No frames are allocated and no mappings are created in the page tables. \
    /// Returns the new [`VirtualMemoryArea`] if successful, otherwise `None`.
    pub fn user_alloc_map_full(&self, start_page: Option<Page>, num_pages: u64, vma_type: VmaType, vma_tag: &str) -> Option<Arc<VirtualMemoryArea>> {
        info!(
            "user_alloc_map_full: start_page: {:?}, num_pages: {}, vma_type: {:?}, vma_tag: {}",
            start_page, num_pages, vma_type, vma_tag
        );
        let vma = self.alloc_vma(start_page, num_pages, MemorySpace::User, vma_type, vma_tag);
        if vma.is_none() {
            return None;
        }
        let vma = vma.unwrap();

        self.page_tables.map(
            vma.range,
            MemorySpace::User,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
        );

        Some(vma)
    }

    /// Manually get the physical address of a virtual address in this address space. \
    pub fn get_phys(&self, virt_addr: u64) -> Option<PhysAddr> {
        self.page_tables.translate(VirtAddr::new(virt_addr))
    }

    /// Copy `total_bytes_to_copy` from `src_ptr` in the `self` address space to `dest_page_start` in the `dest_process` address space. \
    /// Destination addresses are manually retrieved from the page tables of the `dest_process`. \
    /// If `fill_up_with_zeroes` is true, the remaining bytes in the last page will be filled with zeroes.
    pub unsafe fn copy_to_addr_space(
        &self, src_ptr: *const u8, dest_space: &VirtualAddressSpace, dest_page_start: Page, total_bytes_to_copy: u64, fill_up_with_zeroes: bool,
    ) {
        // Calc number of pages to be copied
        let pages_to_copy = if total_bytes_to_copy as usize % PAGE_SIZE == 0 {
            total_bytes_to_copy as usize / PAGE_SIZE
        } else {
            (total_bytes_to_copy as usize / PAGE_SIZE) + 1
        };

        unsafe {
            let mut bytes_to_copy = 0;
            let mut offset = 0;

            let mut dest_phys_addr = dest_space.get_phys(dest_page_start.start_address().as_u64()).expect("get_phys failed");
            let mut dest = dest_phys_addr.as_u64() as *mut u8;
            for _i in 0..pages_to_copy {
                // get destination physical address
                dest_phys_addr = dest_space.get_phys(dest_page_start.start_address().as_u64() + offset).expect("get_phys failed");
                dest = dest_phys_addr.as_u64() as *mut u8;

                // source virtual address
                let source_addr = src_ptr.offset(offset as isize);

                // calc number of bytes to copy
                if total_bytes_to_copy - offset < PAGE_SIZE as u64 {
                    // if we are at the last page, copy only the remaining bytes
                    bytes_to_copy = total_bytes_to_copy - offset;
                } else {
                    bytes_to_copy = PAGE_SIZE as u64;
                }

                // copy code bytes
                dest.copy_from(source_addr, bytes_to_copy as usize);

                offset += bytes_to_copy;
            }

            // fill up last code page with zeroes if not fully used
            if fill_up_with_zeroes {
                let rest_bytes_to_copy = PAGE_SIZE as u64 - bytes_to_copy;
                if rest_bytes_to_copy > 0 {
                    dest.offset(offset as isize).write_bytes(0, rest_bytes_to_copy as usize);
                }
            }
        }
    }
}

impl Drop for VirtualAddressSpace {
    fn drop(&mut self) {
        for vma in self.virtual_memory_areas.read().iter() {
            self.page_tables.unmap(vma.range(), true);
        }
    }
}
