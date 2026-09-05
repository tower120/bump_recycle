[![Crates.io Version](https://img.shields.io/crates/v/bump-recycle)](https://crates.io/crates/bump_recycle)
[![docs.rs](https://img.shields.io/docsrs/bump_recycle)](https://docs.rs/bump_recycle/)
![Crates.io License](https://img.shields.io/crates/l/bump-recycle)

# Recyclable bump allocator

`bump_recycle` is a bump allocator (ala `bumpalo`), that can reuse deallocated
memory blocks.

See [the documentation](https://docs.rs/bump_recycle/) for details.

```rust
let mut alloc = ReBump::new();
let vec1 = Vec::new_in(&alloc);
let vec2 = Vec::new_in(&alloc);

vec1.extend([1,2,3]);
// deallocate memory used by vec1.
vec1.clear();
vec1.shrink_to_fit();

// vec2 now internally reuse memory chunk,
// previously allocated by vec1. No "bump" happens.
vec2.extend([1.0,2.0,3.0]);
```

# Motivation
## `ReBump` against scoped bump

```rust
// let mut bump: Bump = bump_scope::Bump::new();
// let mut alloc = bump.as_mut_scope();

let mut alloc = ReBump::new();
{
    let mut v1 = Vec::new_in(&alloc);
    // This will allocate chunks for [4, 8, 16] items -
    // with only last one used in the end.
    for i in 0..16 {
        v1.push(i);
    }

    // Even thou v1 is not disposed - chunks [4, 8] are free -
    // they will be used in process of filling v2.
    let mut v2 = Vec::new_in(&alloc);
    for i in 0..16{
        v1.push(i);
    }

    // we're end with [4, 8, 16, 16] memory blocks.
}
```
With scoped bump - you would end with `[4,8,16,4,8,16]` memory blocks.
You will reclaim all memory back at the end of the scope - but in a
process you'll need a bigger buffer. That could also mean more allocations.

## `ReBump` as storage

Let's say we need to make some structure that requires Vec of Vec's.
Take as example a naive implementation of graph, DOM, or k-tree.

```rust
struct S<T>{
    data: Vec<Vec<T>>   // patchwork of memory blocks inside
}
```

The problem here - is that each `data` push or swap_remove will allocate/deallocate
memory for the inner Vec - it can't be reused this way.

Now, imagine that we have some storage, that somehow magically could
allocate all our inner Vec's tightly packed together. Let's use allocator concept
for that, also let's imaging the following syntax exists:
```rust
struct S<T>{
    memory: Alloc,  // a few monolithic memory blocks inside
    data: Vec<Vec<T, &'self Alloc>, &'self Alloc>
}
```
Now make it more realistic:
```rust
struct S<T>{
    data: Vec<Vec<T, Rc<Alloc>>, Rc<Alloc>>
}
```
`Rc` will kill structure's `Send`-ability, but as long as `T` and `Alloc` safe,
and we does not expose `Rc`s in any way - we can slap `impl Send` on it.

Replace `Alloc` with `ReBump` - and here we are.

### Why not Object Pool?

With pool of removed inner Vec's you still need to:
1) Allocate NEW inner Vec's one by one.
2) Object pool will return you Vec with ANY capacity previously allocated -
you still may need to allocate again.
3) You obviously can't reuse that memory for other kind of objects.

### Why not common `Vec<T>` + `Vec<Range<usize>>`?

With this pattern, commonly used in CSR graphs - you can't change len
of inner Vec, without rebuilding whole data structure.

### Why not just a "normal" bump allocator inside?

Bump allocator is small and fast - and can be used as a structure memory storage.
But only if it grows only.
As soon as you'll start constantly deleting/adding elements - you'll eventually
run out of memory.

# Overhead

Each "allocation request size" will be aligned to POT.

Only deallocated blocks of the same size as allocation request - can be reused
(both aligned to POT).

# Examples

See [/examples](https://github.com/tower120/bump_recycle/tree/main/examples) folder for usage examples.

# Performance

See [/benches](https://github.com/tower120/bump_recycle/tree/main/benches) folder.

Plain monotonic allocation.

  Type    | ReBump   | bumpalo  |
--------- | -------- | -------- |
`[u64;1]` | 36.8 µs  | 43.4 µs  |
`[u64;2]` | 43.3 µs  | 46.3 µs  |
`[u64;4]` | 67.8 µs  | 67.5 µs  |
`[u64;8]` | 205.3 µs | 223.8 µs |

Allocate, deallocate, then allocate again. Simulates memory reuse.

  Type    | ReBump   | bumpalo  |
--------- | -------- | -------- |
`[u64;1]` | 38.1 µs  | 88.1 µs  |
`[u64;2]` | 49.8 µs  | 94.7 µs  |
`[u64;4]` | 72.2 µs  | 245.1 µs |
`[u64;8]` | 224.7 µs | 855.4 µs |


# Known alternatives

* [bumpalo](https://crates.io/crates/bumpalo)
* [bump_scope](https://crates.io/crates/bump-scope)