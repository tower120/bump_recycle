use std::alloc::Allocator;
use crate::ReBump;

unsafe impl Allocator for ReBump{
    fn allocate(&self, layout: std::alloc::Layout)
        -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError>
    {
        Self::allocate(self, layout)
    }

    unsafe fn deallocate(
        &self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout
    ) {
        unsafe{ Self::deallocate(self, ptr, layout) }
    }
}
