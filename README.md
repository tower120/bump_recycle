# Recyclable bump allocator

`bump_recycle` is a bump allocator (ala `bumpalo`), that can reuse deallocated
memory blocks.

See the documentation for details.

```rust
let mut alloc = ReBump::new();
let vec1 = Vec::new_in(&mut alloc);
let vec2 = Vec::new_in(&mut alloc);

vec1.extend([1,2,3]);
vec1.clear();
vec1.shrink_to_fit();

// vec2 now internally reuse memory chunk,
// previously allocated by vec1. No "bump" happens.
vec2.extend([1.0,2.0,3.0]);
```

# Known alternatives

* [bumpalo](https://crates.io/crates/bumpalo)
* [bump_scope](https://crates.io/crates/bump-scope)