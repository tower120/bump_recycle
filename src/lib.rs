//! [`ReBump`] is much like `bumpalo` - except it reuses deallocated memory.
//!
//! To put it dramatically - you basically don't need nor reset, nor scopes -
//! if you can tolerate all your allocation sizes being aligned to POT,
//! and you allocate/deallocate more or less the same layout sizes.
//!
//! See `/examples` for motivation examples.
//!
//! # How it works
//!
//! All memory allocated in POT size blocks. When you deallocate - that block
//! stored in a linked list of blocks with the same size.
//! When you allocate - [`ReBump`] first look in a table for requested size
//! (aligned to POT) - if there is one - it returns it, if no - it works exactly
//! as bumpalo.
//!
//! ## Design choice
//!
//! [`ReBump`] does not unify or split deallocated blocks - thus it can only reuse
//! blocks of these exact sizes (aligned to POT). This is deliberately to simplify
//! allocation process. Since blocks aligned to POT sizes - when you work with
//! growing `Vec`s - you most likely will have blocks of needed sizes.
//!
//! ## Align
//!
//! All internal chunks and blocks have 8 bytes align.
//! Requested layouts with align > 8 will store additional information block (8 bytes)
//! before the begin of data block.
//!
//! # Maximum size
//!
//! Maximum size this allocator can allocate is `u32::MAX * 8` bytes.
//!
//! Open an issue if you need bigger allocations.
//!
//! # ZST
//!
//! ZST does not handled specially.
//! Allocator will use minimal possible block (ALIGN size) for all ZST allocations.
//! This is to minimize branching.
//! Such behavior does not brake `allocator_api` contract.
//!
//! Open an issue - if you think this should be different.
//!
//! # Features
//!
//! * `allocator_api` - for Rust's [allocator_api](https://doc.rust-lang.org/std/alloc/trait.Allocator.html)
//! support.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(feature = "allocator_api", feature(allocator_api))]

use std::{
    alloc::Layout,
    array::from_fn,
    cell::Cell,
    ptr::{self, NonNull, null_mut},
    cmp, hint,
};

#[cfg(feature = "allocator_api")]
#[cfg_attr(docsrs, doc(cfg(feature = "allocator_api")))]
mod allocator_api;

/// Amount of block classes skipped in `free_blocks` "registry".
const BLOCK_CLASS_SKIP: usize = 3;              // TODO: ALIGN dependent?

/// Align used both for Chunk and for Block.
const ALIGN: usize = 8;

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
    /// Pointer has ALIGN align.
    pub fn data_ptr(this: NonNull<Self>) -> *mut u8 {
        let ptr: *mut u8 = this.as_ptr().cast();
        unsafe{
            ptr.add(size_of::<ChunkHeader>())
        }
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
}

unsafe impl Send for ReBump{}

impl Default for ReBump{
    fn default() -> Self {
        Self::new()
    }
}

impl ReBump{
    pub fn new() -> Self {
        Self {
            root_chunk: Cell::new(NonNull::from(&EMPTY_CHUNK.0)),
            free_blocks: from_fn(|_| const {
                Cell::new(null_mut())
            }),
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

    /// Drops all chunks, but the very last one.
    /// Drops all free blocks.
    ///
    /// In most cases you don't need this - since all freed blocks reused.
    /// You may only need this for some pathological cases -
    /// were you allocated 4Gb in 16 byte layouts, and now want to allocate
    /// the same amount, but in 32 byte layouts - and you will never ever need
    /// to allocate 16 byte layouts again - or tight on memory.
    pub fn reset(&mut self){
        // 1. Drop chunks
        let mut root_chunk = self.root_chunk.get();
        if root_chunk.as_ptr().cast_const() == &EMPTY_CHUNK.0{
            // Do not have a single chunk allocated.
            return;
        }
        // 1.1 Drop chunk chain, starting from prev.
        unsafe{
            let chunk = (root_chunk.as_mut()).prev_chunk;
            Self::drop_chunk_chain(chunk);
        }
        // 1.2 Update root's prev and len.
        unsafe{
            let root_chunk = root_chunk.as_mut();
            root_chunk.prev_chunk = &EMPTY_CHUNK.0 as *const ChunkHeader as * mut ChunkHeader;
            root_chunk.len.set(0);
        }

        // 2. Clear free block "chains".
        for free_block_root in &mut self.free_blocks{
            free_block_root.set(null_mut());
        }
    }

    #[inline]
    pub fn allocate(&self, layout: std::alloc::Layout)
        -> Option<std::ptr::NonNull<[u8]>>
    {
        /* if layout.size() == 0{
            hint::cold_path();
            todo!("ZST!");
        } */

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
fn align_up_ptr<T>(ptr: *mut T, align: usize) -> *mut T {
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

    #[test]
    fn test_zst(){
        const SIZE: usize = 2_000;
        let allocator = ReBump::new();
        // u128 has align 16 - this should force ReBump to use BlockHeader.
        let mut vec: Vec<(), ReBump> = Vec::new_in(allocator);
        for _ in 0..SIZE{
            vec.push(());
        }
        assert_equal(vec.iter().copied(), (0..SIZE).map(|_| ()));
        vec.clear();
        vec.shrink_to_fit();

        // This run should reuse blocks.
        for _ in 0..SIZE{
            vec.push(());
        }
        assert_equal(vec.iter().copied(), (0..SIZE).map(|_| ()));
    }

    #[test]
    fn test_reset(){
        const SIZE: usize = 3_000;
        let mut allocator = ReBump::new();
        allocator.reset();

        {
            let mut vec: Vec<_, &ReBump> = Vec::new_in(&allocator);
            for i in 0..SIZE{
                vec.push(i.to_string());
            }
            assert_equal(vec.iter().cloned(), (0..SIZE).map(|i| i.to_string()));
        }
        allocator.reset();

        let mut vec: Vec<_, &ReBump> = Vec::new_in(&allocator);
        for i in 0..SIZE{
            vec.push(i.to_string());
        }
        assert_equal(vec.iter().cloned(), (0..SIZE).map(|i| i.to_string()));
    }
}