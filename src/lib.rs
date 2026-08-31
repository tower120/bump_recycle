#![cfg_attr(feature = "allocator_api", feature(allocator_api))]
// #![feature(const_array)]

use std::{alloc::Layout, array::from_fn, cell::Cell, cmp, hint, ptr::{self, NonNull, null_mut}};

#[cfg(feature = "allocator_api")]
mod allocator_api;

/// Amount of block classes skipped in `free_blocks` "registry".
pub const BLOCK_CLASS_SKIP: usize = 3;              // TODO: ALIGN dependent?
pub const MAX_SIZE: usize = u32::MAX as usize * 8;  // TODO: use ALIGN?

/// Align used both for Chunk and for Block.
pub const ALIGN: usize = 8;

struct BlockHeader{
    ptr_offset: u32,
    block_class: u32,
}

struct Block{
    next_free: *mut Block,
}

#[repr(align(8))]
struct ChunkHeader{
    prev_chunk: *mut ChunkHeader,
    len: Cell<usize>,
    capacity: usize,
}
impl ChunkHeader{
    /// Pointer has align ALIGN.
    pub fn data_ptr(this: NonNull<Self>) -> *mut u8 {
        let ptr: *mut u8 = this.as_ptr().cast();
        let p = unsafe{
            ptr.add(size_of::<ChunkHeader>())
        };
        p
    }

    // TODO: use allocator
    pub fn allocate_chunk(prev_chunk: *mut ChunkHeader, capacity: usize) -> NonNull<Self> {
        use std::alloc::*;
        let size = size_of::<ChunkHeader>() + capacity;
        let layout = Layout::from_size_align(size, ALIGN).unwrap();
        let ptr = unsafe{ alloc(layout) };
        let this = NonNull::new(ptr.cast()).unwrap();
        unsafe {
            this.write(ChunkHeader{
                prev_chunk,
                len: Cell::new(0),
                capacity,
            });
        }
        this
    }

    #[inline]
    pub unsafe fn deallocate_chunk(this: *mut Self){
        use std::alloc::*;
        let capacity = unsafe{ (*this).capacity };
        let size = size_of::<ChunkHeader>() + capacity;
        let layout = Layout::from_size_align(size, ALIGN).unwrap();
        unsafe{
            dealloc(this.cast(), layout);
        }
    }
}

#[repr(transparent)]
struct EmptyChunkHeader(ChunkHeader);
unsafe impl Sync for EmptyChunkHeader {}

static EMPTY_CHUNK: EmptyChunkHeader = EmptyChunkHeader(ChunkHeader {
    prev_chunk: null_mut(),
    len: Cell::new(0),
    capacity: 0
});

pub struct ReBump{
    root_chunk : Cell<NonNull<ChunkHeader>>,

    /// each block size = 2^index + 8
    free_blocks: [Cell<*mut u8>; 32],

    /// Allocation/deallocation balance. If 0 - everything deallocated.
    alloc_balance: Cell<i64>,
}

impl ReBump{
    pub /* const */ fn new() -> Self {
        Self {
            root_chunk: Cell::new(NonNull::from(&EMPTY_CHUNK.0)),
            free_blocks: from_fn(|_| const {
                Cell::new(null_mut())
            }),
            alloc_balance: Cell::new(0)
        }
    }

    fn pop_free_block(root: &Cell<*mut u8>) -> Option<*mut u8>{
        let root_ptr = root.get();
        if root_ptr.is_null(){
            return None;
        }

        // update root
        {
            let block: *mut Block = root_ptr.cast();
            let next_free = unsafe{(*block).next_free};
            root.set(next_free.cast());
        }

        Some(root_ptr)
    }

    fn push_free_block(root: &Cell<*mut u8>, block_ptr: *mut u8) {
        let root_ptr = root.get();

        // update block
        {
            let block: *mut Block = block_ptr.cast();
            unsafe{(*block).next_free = root_ptr.cast()};
        }

        // set new root
        root.set(block_ptr);
    }

    #[inline]
    fn correct_layout(layout: std::alloc::Layout) -> std::alloc::Layout{
        let align = cmp::max(layout.align(), ALIGN);
        unsafe{
            Layout::from_size_align_unchecked(layout.size(), align)
        }.pad_to_align()
    }

    /// Perfectly aligned data, does not need BlockHeader information.
    #[inline]
    fn perfectly_aligned(layout: std::alloc::Layout) -> bool {
        layout.align() <= ALIGN
    }

    #[inline]
    pub fn allocate(&self, layout: std::alloc::Layout)
        -> Option<std::ptr::NonNull<[u8]>>
    {
        if layout.size() == 0{
            hint::cold_path();
            todo!("ZST!");
        }

        // 0. Correct layout
        let layout = Self::correct_layout(layout);

        // Will be compile-time generated most of the times.
        let perfectly_aligned = Self::perfectly_aligned(layout);
        let block_size_exp = {
            let max_padding_offset = layout.align() - ALIGN;
            let block_header_size = if !perfectly_aligned{size_of::<BlockHeader>()} else {0};
            let block_size = layout.size() + max_padding_offset + block_header_size;
            ilog2_ceil(block_size) as usize    // rounding up
        };
        let block_class_index = block_size_exp - BLOCK_CLASS_SKIP;

        if block_class_index >= 32{
            // Required layout size is too big.
            return None;
        }

        let block_ptr = (||{
            // 1. Try `free_blocks` first
            let free_block_root = unsafe{ self.free_blocks.get_unchecked(block_class_index) };
            if let Some(ptr) = Self::pop_free_block(free_block_root){
                return ptr;
            }

            let mut chunk_ptr = self.root_chunk.get();
            let block_size = 1usize<<block_size_exp;

            // 2. "Allocate" in chunk.
            // 2.1 Check for new chunk allocation.
            {
                let chunk = unsafe{ self.root_chunk.get().as_ref() };
                let requested_len = chunk.len.get() + block_size;
                if requested_len > chunk.capacity{
                    hint::cold_path();

                    // TODO: push biggest possible leftover as a free block.

                    let new_capacity =
                        cmp::max(
                            align_up::<2>(requested_len),
                            chunk.capacity * 2
                        );
                    chunk_ptr = ChunkHeader::allocate_chunk(self.root_chunk.get().as_ptr(), new_capacity);
                    self.root_chunk.set(chunk_ptr);
                }
            }

            // 2.2 Update len and return data pointer.
            let chunk = unsafe{ self.root_chunk.get().as_mut() };

            let start = chunk.len.get();
            chunk.len.set(start + block_size);

            let data_ptr = ChunkHeader::data_ptr(chunk_ptr);
            unsafe{ data_ptr.add(start) }
        })();

        let ptr = if perfectly_aligned{
            // Don't add BlockHeader if we're perfectly aligned.
            block_ptr
        } else {
            let ptr = align_up_ptr(unsafe{ block_ptr.add(size_of::<BlockHeader>()) }, layout.align());
            // Write BlockHeader
            unsafe {
                let block_header_ptr: *mut BlockHeader = ptr.sub(size_of::<BlockHeader>()).cast();
                block_header_ptr.write(BlockHeader {
                    ptr_offset : ptr.offset_from(block_ptr) as u32,
                    block_class: block_class_index as u32
                });
            }
            ptr
        };

        self.alloc_balance.update(|i| i+1);

        let slice = ptr::slice_from_raw_parts_mut(ptr, layout.size());
        return Some(unsafe{NonNull::new_unchecked(slice)})
    }

    pub unsafe fn deallocate(
        &self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout
    ) {
        let (block_class_index, block_ptr) = if Self::perfectly_aligned(layout){
            // We're perfectly aligned, we don't have BlockHeader.
            // Calculate size that we used.
            let layout = Self::correct_layout(layout);
            let block_size_exp = {
                let block_size = layout.size();
                ilog2_ceil(block_size) as usize    // rounding up
            };
            let block_class_index = block_size_exp - BLOCK_CLASS_SKIP;
            (block_class_index, ptr.as_ptr())
        } else {
            unsafe{
                let ptr = ptr.as_ptr();
                let block_header_ptr: *const BlockHeader = ptr.sub(size_of::<BlockHeader>()).cast();
                let block_class_index = (*block_header_ptr).block_class as usize;
                let block_ptr = ptr.sub((*block_header_ptr).ptr_offset as usize);
                (block_class_index, block_ptr)
            }
        };

        let free_block_root = unsafe{ self.free_blocks.get_unchecked(block_class_index) };
        Self::push_free_block(free_block_root, block_ptr);

        self.alloc_balance.update(|i| i-1);
    }

    #[inline]
    unsafe fn drop_chunk_chain(mut chunk_head_ptr: *mut ChunkHeader){
        while chunk_head_ptr.cast_const() != &EMPTY_CHUNK.0 {
            let next_chunk_head_ptr = {
                let chunk = unsafe{ chunk_head_ptr.as_ref_unchecked() };
                chunk.prev_chunk
            };
            unsafe{
                ChunkHeader::deallocate_chunk(chunk_head_ptr);
            }
            chunk_head_ptr = next_chunk_head_ptr
        }
    }
}

impl Drop for ReBump{
    #[inline]
    fn drop(&mut self) {
        unsafe{
            let root_chunk_ptr = self.root_chunk.get().as_ptr();
            Self::drop_chunk_chain(root_chunk_ptr);
        }
    }
}

fn align_up<const I: usize>(n: usize) -> usize {
    const{ assert!(I.is_power_of_two()) }
    (n + (I-1)) & !(I-1)
}

/// Aligns a raw pointer UP to the nearest multiple of `align`.
/// `align` MUST be a power of two (e.g., 1, 2, 4, 8, 16...).
pub fn align_up_ptr<T>(ptr: *mut T, align: usize) -> *mut T {
    let new_addr = align_up_addr(ptr.addr(), align);
    ptr.with_addr(new_addr)
}

fn align_up_addr(addr: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    let align_min_1 = align - 1;
    (addr + align_min_1) & !(align_min_1)
}

/// MUST be > 1
#[inline]
fn ilog2_ceil(x: usize) -> u32 {
    debug_assert!(x > 1);
    if x <= 1 {
        unsafe { std::hint::unreachable_unchecked() }
    }
    (x - 1).ilog2() + 1
}

#[cfg(test)]
mod tests{
    use super::*;
    use itertools::assert_equal;

    #[test]
    fn test(){
        let allocator = ReBump::new();
        let mut vec: Vec<i32, ReBump> = Vec::new_in(allocator);
        // vec.reserve(100);
        // vec.extend(0..80);
        for i in 0..80{
            vec.push(i);
        }
        assert_equal(vec, 0..80);
        println!("OK");
    }

    #[test]
    fn test_block_reuse(){
        const SIZE: usize = 32_000;
        let allocator = ReBump::new();
        let mut vec: Vec<_, ReBump> = Vec::new_in(allocator);
        for i in 0..SIZE{
            vec.push(i);
        }
        assert_equal(vec.iter().copied(), 0..SIZE);
        vec.clear();
        vec.shrink_to_fit();

        // This run should reuse blocks.
        for i in 0..SIZE{
            vec.push(i);
        }
        assert_equal(vec.iter().copied(), 0..SIZE);
    }

    #[test]
    fn test_block_reuse_w_header(){
        const SIZE: u128 = 32_000;
        let allocator = ReBump::new();
        // u128 has align 16 - this should force ReBump to use BlockHeader.
        let mut vec: Vec<u128, ReBump> = Vec::new_in(allocator);
        for i in 0..SIZE{
            vec.push(i);
        }
        assert_equal(vec.iter().copied(), 0..SIZE);
        vec.clear();
        vec.shrink_to_fit();

        // This run should reuse blocks.
        for i in 0..SIZE{
            vec.push(i);
        }
        assert_equal(vec.iter().copied(), 0..SIZE);
    }
}
