use alloc::collections::BTreeMap;
use core::alloc::Layout;
use core::cmp::min;
use core::ffi::{c_size_t, c_void};
use core::ptr;
use core::ptr::NonNull;
use spin::Mutex;
use runtime::ALLOCATOR;

static ALLOCATIONS: Mutex<BTreeMap<u64, c_size_t>> = Mutex::new(BTreeMap::new());

#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: c_size_t) -> *mut c_void {
    let layout = Layout::from_size_align(size, 8).unwrap();

    let block = ALLOCATOR.lock().allocate_first_fit(layout);
    match block {
        Ok(ptr) => {
            ALLOCATIONS.lock().insert(ptr.as_ptr() as u64, size);
            ptr.as_ptr() as *mut c_void
        },
        Err(_) => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn calloc(num: c_size_t, size: c_size_t) -> *mut c_void {
    unsafe {
        let ptr = malloc(num * size);
        if !ptr.is_null() {
            ptr::write_bytes(ptr, 0, num * size);
        }

        ptr
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, new_size: c_size_t) -> *mut c_void {
    unsafe {
        let new_ptr = malloc(new_size);
        if ptr.is_null() || new_ptr.is_null() {
            return new_ptr;
        }

        let old_size = *ALLOCATIONS.lock()
            .get(&(ptr as u64))
            .expect("realloc: Invalid pointer");

        let copy_size = min(old_size, new_size);
        new_ptr.copy_from_nonoverlapping(ptr, copy_size);
        free(ptr);

        new_ptr
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    let size = *ALLOCATIONS.lock()
        .get(&(ptr as u64))
        .expect("realloc: Invalid pointer");

    let layout = Layout::from_size_align(size, 8).unwrap();
    unsafe { ALLOCATOR.lock().deallocate(NonNull::new_unchecked(ptr as *mut u8), layout); }
}