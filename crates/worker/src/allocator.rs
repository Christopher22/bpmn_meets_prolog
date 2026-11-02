//! A global allocator that tolerates freeing a null pointer.
//!
//! Scryer Prolog 0.10.0 deallocates heaps that were never allocated
//! (`Heap::clear` on an empty heap passes a null pointer to `dealloc`, which
//! upstream fixed after the release). Natively the system allocator ignores
//! `free(NULL)`, but wasm's dlmalloc reads the chunk header in front of the
//! pointer, at address `0xFFFF_FFFC`, and traps. This wrapper drops such
//! calls and forwards everything else unchanged.
//!
//! Remove it once a Scryer release with the fix is out.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};

struct NullTolerant;

// SAFETY: every method forwards to `System` with the caller's arguments,
// except that `dealloc` on a null pointer (which never came from `alloc`)
// does nothing.
unsafe impl GlobalAlloc for NullTolerant {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !ptr.is_null() {
            // SAFETY: a non-null `ptr` was allocated by `System` via `alloc`.
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: NullTolerant = NullTolerant;
