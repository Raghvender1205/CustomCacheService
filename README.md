# Custom Cache System
A rust based cache service.

## Features
Currently, the cache service has 
1. Basic Key-value operations like `GET`, `SET`, `DELETE`
2. Some advanced operations
    
    - Expire :- It sets an expiration time on a key
    - Incr :- Increments an integer value stored at a specific key
    - Decr :- Decrements an integer value stored at a specific key
    - Keys :- List all keys in the cache that match a specified pattern using regex.
3. Uses `Mutex` to ensure all operations on the cache are thread-safe.
4. TTL on keys.
5. LRU Eviction policy
6. Logging and monitoring.

## Design Approach
1. Maybe use `Hashmaps` for key-value storage.
2. Add concurrency using `Arc`, `Mutex`
3. Add cache operations like `SET`, `GET`, `DELETE` and `EXPIRE`
4. Ensure thread safety and atomicity of operations
5. Choose either JSON/MessagePack for sending and receiving data over network.
6. Implement TTL for keys support automatic expiration
7. Maybe use LRU as the eviction policy when `cache` is full.
8. Maybe add `Persistence`, save and load the cache state.

## TODO
1. Binary Protocol :- For more efficient communication, maybe shift from JSON to a binary protocol.
2. Add TLS encryption 
3. Persistence 