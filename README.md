# Custom Cache System
A rust based cache service.

## Design Approach
1. Maybe use `Hashmaps` for key-value storage.
2. Add concurrency using `Arc`, `Mutex`
3. Add cache operations like `SET`, `GET`, `DELETE` and `EXPIRE`
4. Ensure thread safety and atomicity of operations
5. Choose either JSON/MessagePack for sending and receiving data over network.
6. Implement TTL for keys support automatic expiration
7. Maybe use LRU as the eviction policy when `cache` is full.
8. Maybe add `Persistence`, save and load the cache state.