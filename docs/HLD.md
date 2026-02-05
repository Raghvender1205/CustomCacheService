<!-- docs/HLD.md -->

# Distributed Cache Service (Rust) — High-Level Design (HLD)

## 1. Overview

This project is a **distributed, high-throughput, in-memory cache** implemented in Rust with:

- **Shared-nothing, shard-per-core** execution for predictable scaling on multicore machines.
- **Pluggable eviction policies** (LRU, SIEVE, FIFO, Random; extensible).
- **TTL support** with efficient expiry handling.
- **Redis-protocol compatibility mode (RESP2/RESP3)** so the service can be used via Redis clients and GUI tools such as Redis Insight.
- **Cluster mode** with sharding + optional replication.

### Why this design
- Traditional single-lock designs (global mutex around the whole store) serialize work and cap throughput.
- High-performance caches avoid global locks by **partitioning state into independent shards** and ensuring requests for a key are handled by exactly one shard thread.
- Eviction policies must be designed with **hit-path cost** in mind: per-hit bookkeeping can become the bottleneck under read-heavy workloads.

---

## 2. Goals and Non-goals

### Goals
1. **Max throughput / low tail latency**
   - No global locks on hot path.
   - Avoid work on cache hits where possible.
2. **Horizontal scalability**
   - Add nodes → higher total throughput and capacity.
3. **Eviction policy as a “plugin”**
   - Choose policy at startup and/or per-namespace.
4. **Redis Insight compatibility**
   - Speak RESP (Redis protocol), implement the minimal command subset that Redis Insight relies on for discovery and browsing.
5. **Operational visibility**
   - Metrics, tracing/logs, health endpoints.

### Non-goals (initially)
- Full Redis feature parity (Lua, modules, complex data types).
- Strong durability guarantees (this is a cache; persistence is optional later).
- Cross-datacenter strong consistency.

---

## 3. System Architecture

### 3.1 Components

**Client options**
- **Smart client / SDK** (recommended): computes shard node via consistent hashing/jump hashing.
- **Proxy/router** (optional): accepts requests and routes to nodes (adds hop; can bottleneck unless scaled).

**Cache Node**
- Network front-end (RESP + optional native protocol)
- Request parsing, routing to local shard worker
- Storage engine (shards)
- Background tasks (expiry cleanup, replication, metrics)

**Control Plane (Cluster Manager)**
- Membership and failure detection
- Cluster configuration distribution (ring version)
- Optional coordinator (Raft/etcd) or gossip-based membership

### 3.2 Data Plane vs Control Plane

- **Data plane**: GET/SET/DEL/SCAN/TTL etc.
- **Control plane**: node joins/leaves, ring updates, reconfiguration, health.

---

## 4. Protocols & APIs

### 4.1 Redis Compatibility Mode (RESP)

The service exposes a **RESP endpoint** (default `:6379`) and supports:
- RESP2 by default
- Optional RESP3 negotiation via `HELLO 3`

**Why this matters:** Redis Insight connects to servers using Redis commands and expects at minimum that the connected user can run `INFO` for discovery and stats. Also, most tooling assumes standard RESP request/response framing.

#### Minimal command set (Phase 1: Redis Insight friendly)
**Connectivity & handshake**
- `PING`
- `HELLO [2|3]`
- `AUTH` (optional)
- `COMMAND` or `COMMAND INFO/LIST` (minimal, for CLI usability/autocomplete)
- `CLIENT SETNAME` (optional)

**Key/value (Strings)**
- `GET`, `SET`, `DEL`
- `MGET`, `MSET` (batching)
- `INCR`, `DECR`

**TTL**
- `EXPIRE`, `TTL`, `PTTL`
- `PERSIST` (optional)

**Keyspace iteration**
- `SCAN` (preferred)
- `KEYS` (optional; strongly discouraged in production — implement only if you want, and gate it)

**Stats / admin**
- `INFO` (critical for Insight)
- `DBSIZE`
- `CONFIG GET` (restricted; for read-only config visibility)
- `MEMORY USAGE` (approx, optional)

> Note: If you implement only one key type initially, return `TYPE key => "string"` for existing keys.

### 4.2 Native Protocol (Optional “performance port”)

To push throughput further, you can optionally keep your current binary protocol (or adopt a framed format like:
`[u32 len][payload]`) on another port (e.g., `:6380`).

This lets you:
- Keep Redis compatibility for tools
- Offer a leaner, faster protocol for your own clients

---

## 5. Data Model

Each entry stores:

- `key: Bytes` (or `String` initially)
- `value: Bytes`
- `meta`:
  - `expire_at: Option<Instant>`
  - `size_bytes: usize`
  - `version: u64` (optional; helpful for replication & correctness)
  - Policy-specific metadata (e.g., LRU pointers, SIEVE flags)

---

## 6. Node Internals (Single-node throughput)

### 6.1 Sharding strategy (intra-node)

- `num_shards = num_cores` (or tuned)
- `shard_id = hash(key) % num_shards`
- Each shard owns its **own hashmap + eviction state + TTL structures**.

### 6.2 Execution model

**Recommended**: “Shard-per-core workers”
- A small number of **I/O threads** accept sockets and parse frames.
- Each request is routed to exactly one shard worker via an MPSC channel.
- Shard worker executes command without locks and returns response.

Benefits:
- Avoids mutex contention
- Predictable latency under load
- Easy per-shard metrics

### 6.3 Storage structures

Per shard:
- `HashMap<Key, Entry>`
- `EvictionPolicy` state
- `ExpiryIndex` (timing wheel or min-heap)
- `Stats`

---

## 7. Eviction Policy Framework

### 7.1 Trait interface

Define an `EvictionPolicy` trait (conceptually):

- `on_insert(key, entry_meta)`
- `on_access(key)` (optional; some policies are “hit-light”)
- `on_remove(key)`
- `evict_until(used_bytes <= max_bytes)` → list of evicted keys

### 7.2 Policies

**LRU**
- Maintains strict recency.
- Cost: per-hit bookkeeping (list operations).

**SIEVE**
- Designed to be simpler and scalable; avoids heavy work on hits.
- Excellent for multi-thread / high-core environments.

**FIFO / Random**
- Very cheap, good for baseline.

### 7.3 Memory limit policy

Prefer **byte-based limits**:
- `max_bytes` per shard (derived from node limit / shard count)
- Optional `max_items` for guardrails

---

## 8. TTL / Expiration

Avoid global scans.

Approaches:
1. **Lazy expiration** on access:
   - On `GET`, check expiry and delete if expired.
2. **Background expiry** per shard:
   - Timing wheel (O(1) amortized)
   - or Min-heap (O(log n))

Implementation detail:
- Expiry index may contain stale entries; validate version/expire_at on pop.

---

## 9. Distributed Mode

### 9.1 Key placement (inter-node)

Two common options:
- **Consistent hashing ring** with virtual nodes
- **Jump consistent hash** when buckets are stable and numbered

Flow:
- `primary_node = placement(key)`
- `replica_nodes = next R-1 nodes`

### 9.2 Replication

Config:
- `replication_factor = R`
- `write_policy = async | quorum`

**Async replication (default cache choice)**
- Client writes to primary
- Primary ACKs immediately
- Primary replicates in background

**Quorum replication**
- ACK after W of R confirm
- Higher latency, stronger availability semantics

### 9.3 Failure handling

- Client/router detects node down via health checks.
- Route to next replica.
- Ring version bump on membership change.

### 9.4 Rebalancing

Caches often accept cold misses after re-shard.
Optional enhancement:
- Warm-up migration for hot keys / namespaces.
- Background streaming transfer.

---

## 10. Observability

### 10.1 Metrics
Per shard:
- `ops_total`, `get_hits`, `get_misses`
- `evictions_total`, `expired_total`
- `bytes_used`, `items`
- p50/p95/p99 latency histograms

Node:
- connections, queue depth, CPU/memory, ring version

Expose:
- Prometheus `/metrics`
- `/healthz`, `/readyz`
- `/debug/ring`

### 10.2 Tracing & profiling
- Structured logs
- Optional tracing (OpenTelemetry)
- Built-in benchmark endpoints (optional)

---

## 11. Security

- TLS / mTLS (optional)
- AUTH / ACL-like restrictions for command categories
- Namespace separation:
  - `tenant:namespace:key` prefixing OR internal namespace field

---

## 12. Deployment

- Single binary per node
- Config via file/env:
  - bind addr/ports
  - max memory
  - shards
  - eviction policy
  - cluster seed nodes
  - replication factor
  - auth settings

---

## 13. Testing & Benchmarking

### 13.1 Testing
- Unit tests: eviction correctness, TTL correctness, protocol framing
- Property tests: random sequences of ops
- Integration:
  - `redis-cli` against RESP port
  - Redis Insight manual smoke test
  - multi-node cluster tests (docker-compose)

### 13.2 Benchmark plan
- `GET` hit throughput
- `SET` throughput with eviction
- mixed workloads (90/10 read/write)
- TTL-heavy workloads
- compare policies LRU vs SIEVE

---

## 14. Migration from current code

Current issues to address:
- Global `Mutex<DataStore>` becomes a throughput bottleneck.
- TCP framing: fixed 1024 reads assume message boundaries (unsafe).

Migration steps:
1. Add a framed protocol layer (RESP + optional binary).
2. Introduce shards and route by key hash.
3. Move eviction into `EvictionPolicy` trait.
4. Replace global cleanup scan with per-shard expiry index.
5. Add cluster placement layer.

---

## 15. Milestones

**M0**: Single-node, shard-per-core, RESP2 support, strings + TTL + SCAN  
**M1**: Eviction plugins: LRU + SIEVE + FIFO  
**M2**: Redis Insight compatibility polish (INFO, COMMAND, CONFIG GET limited)  
**M3**: Cluster mode sharding + async replication  
**M4**: Production hardening (TLS, ACL, observability, chaos tests)
