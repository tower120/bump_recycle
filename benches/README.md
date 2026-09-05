# Benchmark results

Test machine: `i7-4771; 1600Mhz DDR3; Windows 10`.

## Allocation

From `alloc.rs`. Plain monotonic allocation.

  Type    | ReBump   | bumpalo  |
--------- | -------- | -------- |
`[u64;1]` | 36.8 µs  | 43.4 µs  |
`[u64;2]` | 43.3 µs  | 46.3 µs  |
`[u64;4]` | 67.8 µs  | 67.5 µs  |
`[u64;8]` | 205.3 µs | 223.8 µs |

## Allocation, Deallocation, Allocation

From `alloc_dealloc_alloc.rs`. Allocate, deallocate, then allocate again.
Simulates memory reuse.

  Type    | ReBump   | bumpalo  |
--------- | -------- | -------- |
`[u64;1]` | 38.1 µs  | 88.1 µs  |
`[u64;2]` | 49.8 µs  | 94.7 µs  |
`[u64;4]` | 72.2 µs  | 245.1 µs |
`[u64;8]` | 224.7 µs | 855.4 µs |

You can achieve similar result with scoped bump - but you're
hard limited by scope.