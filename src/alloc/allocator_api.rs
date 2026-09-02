/// Wrapper for [`std::alloc::Allocator`]-compatible allocator.
///
/// Allow to use it as allocator for [`ReBump`].
///
/// [`ReBump`]: crate::ReBump
#[repr(transparent)]
pub struct AllocatorApiStd<T: std::alloc::Allocator>(pub T);

unsafe impl<T: std::alloc::Allocator> crate::alloc::Allocator for AllocatorApiStd<T>{
    #[inline]
    unsafe fn allocate_non_zst(&self, layout: std::alloc::Layout) -> Option<std::ptr::NonNull<[u8]>> {
        if layout.size() == 0 {
            unsafe { std::hint::unreachable_unchecked() }
        }
        std::alloc::Allocator::allocate(&self.0, layout).ok()
    }

    #[inline]
    unsafe fn deallocate_non_zst(&self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout) {
        if layout.size() == 0 {
            unsafe { std::hint::unreachable_unchecked() }
        }
        unsafe{
            std::alloc::Allocator::deallocate(&self.0, ptr, layout)
        }
    }
}