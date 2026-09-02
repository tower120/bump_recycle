use std::alloc::Allocator;
use crate::ReBump;

unsafe impl<Alloc: crate::alloc::Allocator> Allocator for ReBump<Alloc>{
    #[inline(always)]
    fn allocate(&self, layout: std::alloc::Layout)
        -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError>
    {
        Self::allocate(self, layout).ok_or(std::alloc::AllocError)
    }

    #[inline(always)]
    unsafe fn deallocate(
        &self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout
    ) {
        unsafe{ Self::deallocate(self, ptr, layout) }
    }
}