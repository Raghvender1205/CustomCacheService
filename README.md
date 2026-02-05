<!-- README.md -->

# CacheService — Distributed Cache in Rust (Redis Insight compatible)

A high-throughput, distributed in-memory cache written in Rust with:
- **Shard-per-core** architecture for multicore scalability
- **Pluggable eviction policies** (LRU, SIEVE, FIFO, Random; extensible)
- **TTL support**
- **Redis protocol (RESP) compatibility** so you can use **redis-cli** and **Redis Insight**
- Optional **cluster mode** (sharding + replication)

> This is a cache service, not a full Redis replacement. The goal is a fast, scalable KV cache with a Redis-compatible interface for tooling.

---

## Why this exists

Traditional cache servers often hit scalability limits due to:
- single-threaded command execution
- shared cache state protected by centralized locks

CacheService is designed around **shared-nothing shards** and eviction policies that keep the **hit path** cheap.

---

## Features

### Current / Planned
- [x] GET/SET/DEL
- [x] INCR/DECR
- [x] TTL (EXPIRE, TTL/PTTL)
- [x] Key iteration (SCAN)
- [x] Pluggable eviction framework
- [ ] LRU policy (baseline)
- [ ] SIEVE policy (high-throughput)
- [ ] Cluster mode (consistent hashing / jump hash)
- [ ] Replication (async, optional quorum)
- [ ] TLS/mTLS
- [ ] Persistence (optional)

---

## Architecture (high level)

- **Front-end**
  - RESP server on `:6379` (Redis-compatible)
  - Optional native binary protocol on `:6380` (future)

- **Storage engine**
  - `N` shards (typically = CPU cores)
  - each shard owns:
    - hashmap
    - eviction state
    - TTL index

- **Cluster (optional)**
  - key → node placement (consistent hashing / jump hash)
  - replication factor R

See: `docs/HLD.md`

---

## Getting started

### Requirements
- Rust stable toolchain

### Build
```bash
cargo build --release
```

### Run
```
RUST_LOG=info cargo run -- --bind 0.0.0.0:6379 --shards 8 --max-items 100000 --eviction sieve
```

# Usage with redis-cli
If you expose RESP on port 6379, you can use standard Redis tooling
```bash
redis-cli -h 127.0.0.1 -p 6379 PING
redis-cli -h 127.0.0.1 -p 6379 SET hello world
redis-cli -h 127.0.0.1 -p 6379 GET hello
redis-cli -h 127.0.0.1 -p 6379 EXPIRE hello 60
redis-cli -h 127.0.0.1 -p 6379 TTL hello
```

Key iteration:
```bash
redis-cli -h 127.0.0.1 -p 6379 SCAN 0 MATCH user:* COUNT 100
```

## Using with Redis Insight 
1. Start `CacheService` with the RESP endpoint enabled (default: `:6379`)
2. Open Redis Insight -> `Add Redis Database`
3. Enter:
- Host: `server_ip`
- Port: `6379`
- Username/password if enabled
4. Connect

### Important
Redis insight uses Redis commands for discovery and browsing (especially `INFO` and key iteration commands like `SCAN`). Ensure the configured user is allowed to execute `INFO` and related introspection commands.

## Redis Compatibility scope
### Supported (target baseline)
- Connection: `PING`, `HELLO`, `AUTH`(optional)
- Strings: `GET`, `SET`, `MGET`, `DEL`, `INCR`, `DECR`
- TTL: `EXPIRE`, `TTL`, `PTTL`, `PERSIST` (optional)
- Iteration: `SCAN` (preferred), `KEYS` (optional)
- Introspection: `INFO`, `DBSIZE`, `COMMAND` (minimal), `CONFIG GET` (limited)

### Not supported (Initially)
- Redis modules (JSON, Search, TimeSeries)
- Lists/Sets/Hashes/ZSets/Streams
- Lua scripting 
- Transactions
- Full cluster protocol parity


## Repo Structure (suggested)
```bash
src/
  main.rs
  config/
  net/
    resp/
      codec.rs
      parser.rs
      writer.rs
      commands.rs
    native/           # optional fast protocol
  engine/
    mod.rs
    shard.rs
    entry.rs
    ttl.rs
    stats.rs
  eviction/
    mod.rs
    lru.rs
    sieve.rs
    fifo.rs
    random.rs
  cluster/
    placement.rs
    membership.rs
    replication.rs
  admin/
    http.rs
tests/
docs/
  HLD.md
```

## Development Plan
### Phase 1 (single node, fast)
- Replace global mutex with shard-per-core
- Implement RESP framing + command parsing
- Implement base commands + TTL + SCAN
- Add LRU + SIEVE policies

### Phase 2 (distributed)
- Cluster membership + placement 
- Async replication
- Reconfiguration and failure handling 

### Phase 3 (hardening)
- TLS/mTLS
- ACL-like permissions
- Prometh