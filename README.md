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

### Binary Protocol
Each command starts with a single byte indicating the command type. The commands are

```
0x01: SET
0x02: GET
0x03: DELETE
0x04: EXPIRE
0x05: INCR
0x06: DECR
0x07: KEYS
```

## TODO
1. Binary Protocol :- For more efficient communication, maybe shift from JSON to a binary protocol.
2. Add TLS encryption 
3. Persistence 
4. Advanced eviction policies like `LFU`, `FIFO` etc.
5. Batch operations to allow client send multiple commands in a single request.
6. Allow versioning of keys so that clients can retrieve specific versions of keys.
7. Provide support for custom serialization formats like `BSON`, `MessagePack` to optimize data storage and transmission
8. Implement rate limiting to control the number of requests a client can make in a given time frame.