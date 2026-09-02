#[cfg(feature = "allocator_api")]
#[cfg_attr(docsrs, doc(cfg(feature = "allocator_api")))]
mod allocator_api;
#[cfg(feature = "allocator_api")]
#[cfg_attr(docsrs, doc(cfg(feature = "allocator_api")))]
pub use allocator_api::*;

use std::{alloc::{Layout}, ptr::NonNull};

pub unsafe trait Allocator{
    /// Layout must be non-ZST.
    unsafe fn allocate_non_zst(&self, layout: Layout) -> Option<NonNull<[u8]>>;

    /// Layout must be non-ZST.
    unsafe fn deallocate_non_zst(&self, ptr: NonNull<u8>, layout: Layout);
}

/// Default Rust allocator.
#[derive(Default, Clone, Copy)]
pub struct Global;
unsafe impl Allocator for Global{
    #[inline]
    unsafe fn allocate_non_zst(&self, layout: Layout) -> Option<NonNull<[u8]>> {
        unsafe{
            let ptr = std::alloc::alloc(layout);
            if ptr.is_null(){
                return None;
            }
            Some(NonNull::new_unchecked(core::ptr::slice_from_raw_parts_mut(
                ptr,
                layout.size(),
            )))
        }
    }

    #[inline]
    unsafe fn deallocate_non_zst(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { std::alloc::dealloc(ptr.as_ptr(), layout) }
    }
}