# MemPool
A Rust library made for fast, parallel friendly memory pool with ability to go from modifiable RAM to shareable (unmodifiable) RAM with Atomic Reference Counters and then back to modifiable RAM which will then be stored at the pool to be re utilized.

Utilizes a bucketed approach to reduce contention on the memory pool (heavy multi threading capable/optimized). This library is not recommended for people which just want a single threaded simple pool. It is meant for high performance environments where malloc might actually cause lots of slowdowns.
