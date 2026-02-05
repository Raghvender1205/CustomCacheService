use bytes::Bytes;
use hashbrown::HashMap;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{self, Duration, Instant};
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::policy::{EvictionKind, Evictor};
use crate::resp::Resp;

// Keyslot hashing (Redis cluster style): stable mapping for routing keys
use redis_protocol::redis_keyslot;

#[derive(Clone)]
pub struct Engine {
    cfg: Config,
    shard_txs: Vec<mpsc::Sender<ShardReq>>,
}

impl Engine {
    pub async fn new(cfg: Config) -> Self {
        let mut shard_txs = Vec::with_capacity(cfg.shards);

        for shard_id in 0..cfg.shards {
            let (tx, rx) = mpsc::channel::<ShardReq>(4096);
            shard_txs.push(tx);

            let max_items = cfg.max_items;
            let eviction = cfg.eviction.clone();
            tokio::spawn(async move {
                Shard::run(shard_id, max_items, &eviction, rx).await;
            });
        }

        // Periodic cleanup tick: ask each shard to expire-scan (slow path)
        let cleanup_secs = cfg.cleanup_secs;
        let cleanup_txs = shard_txs.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(cleanup_secs));
            loop {
                interval.tick().await;
                for tx in &cleanup_txs {
                    let _ = tx.send(ShardReq::Cleanup).await;
                }
            }
        });

        Self { cfg, shard_txs }
    }

    fn shard_index_for_key(&self, key: &Bytes) -> usize {
        let slot = redis_keyslot(key.as_ref());
        (slot as usize) % self.shard_txs.len()
    }

    async fn call_shard(&self, idx: usize, cmd: ShardCmd) -> Resp {
        let (tx, rx) = oneshot::channel();
        if self.shard_txs[idx].send(ShardReq::Call { cmd, resp: tx }).await.is_err() {
            return Resp::Error("ERR shard unavailable".into());
        }
        rx.await.unwrap_or_else(|_| Resp::Error("ERR shard timeout".into()))
    }

    pub async fn get(&self, key: Bytes) -> Resp {
        let idx = self.shard_index_for_key(&key);
        self.call_shard(idx, ShardCmd::Get { key }).await
    }

    pub async fn set(&self, key: Bytes, value: Bytes, px_ms: Option<u64>) -> Resp {
        let idx = self.shard_index_for_key(&key);
        self.call_shard(idx, ShardCmd::Set { key, value, px_ms }).await
    }

    pub async fn del(&self, keys: Vec<Bytes>) -> Resp {
        // multi-key: route per key, sum integer replies
        let mut deleted = 0i64;
        for k in keys {
            let idx = self.shard_index_for_key(&k);
            if let Resp::Integer(n) = self.call_shard(idx, ShardCmd::Del { key: k }).await {
                deleted += n;
            }
        }
        Resp::Integer(deleted)
    }

    pub async fn exists(&self, keys: Vec<Bytes>) -> Resp {
        let mut count = 0i64;
        for k in keys {
            let idx = self.shard_index_for_key(&k);
            if let Resp::Integer(n) = self.call_shard(idx, ShardCmd::Exists { key: k }).await {
                count += n;
            }
        }
        Resp::Integer(count)
    }

    pub async fn expire_secs(&self, key: Bytes, secs: u64) -> Resp {
        let idx = self.shard_index_for_key(&key);
        self.call_shard(idx, ShardCmd::Expire { key, ttl_ms: secs * 1000 }).await
    }

    pub async fn ttl(&self, key: Bytes, ms: bool) -> Resp {
        let idx = self.shard_index_for_key(&key);
        self.call_shard(idx, ShardCmd::Ttl { key, ms }).await
    }

    pub async fn dbsize(&self) -> Resp {
        let mut total = 0i64;
        for i in 0..self.shard_txs.len() {
            if let Resp::Integer(n) = self.call_shard(i, ShardCmd::DbSize).await {
                total += n;
            }
        }
        Resp::Integer(total)
    }

    // ---------------- Redis Insight friendliness ----------------

    pub async fn hello(&self, _args: Vec<Bytes>) -> Resp {
        // RESP "HELLO" reply is a list of properties. Keep it minimal.
        // We keep RESP2 responses for now.
        Resp::Array(vec![
            Resp::Bulk(Some(Bytes::from_static(b"server"))),
            Resp::Bulk(Some(Bytes::from_static(b"cache_service"))),
            Resp::Bulk(Some(Bytes::from_static(b"version"))),
            Resp::Bulk(Some(Bytes::from(self.cfg.clone().bind))), // not true version, but fine for milestone
            Resp::Bulk(Some(Bytes::from_static(b"proto"))),
            Resp::Integer(2),
        ])
    }

    pub async fn client(&self, args: Vec<Bytes>) -> Resp {
        // Support: CLIENT SETINFO ... => OK
        if args.is_empty() {
            return Resp::Error("ERR wrong number of arguments for 'client' command".into());
        }
        let sub = args[0].as_ref();
        if eq_icase(sub, b"SETINFO") {
            return Resp::Simple("OK");
        }
        if eq_icase(sub, b"INFO") {
            return Resp::Bulk(Some(Bytes::from_static(b"id=1 addr=unknown")));
        }
        Resp::Error("ERR unsupported CLIENT subcommand".into())
    }

    pub async fn command(&self, _args: Vec<Bytes>) -> Resp {
        // Minimal "COMMAND" listing used by some clients during handshake.
        // Format is complex; for milestone we return a flat list of names (cheap).
        Resp::Array(vec![
            Resp::Bulk(Some(Bytes::from_static(b"get"))),
            Resp::Bulk(Some(Bytes::from_static(b"set"))),
            Resp::Bulk(Some(Bytes::from_static(b"del"))),
            Resp::Bulk(Some(Bytes::from_static(b"exists"))),
            Resp::Bulk(Some(Bytes::from_static(b"expire"))),
            Resp::Bulk(Some(Bytes::from_static(b"ttl"))),
            Resp::Bulk(Some(Bytes::from_static(b"pttl"))),
            Resp::Bulk(Some(Bytes::from_static(b"scan"))),
            Resp::Bulk(Some(Bytes::from_static(b"info"))),
            Resp::Bulk(Some(Bytes::from_static(b"hello"))),
        ])
    }

    pub async fn info(&self, _args: Vec<Bytes>) -> Resp {
        // Return "key:value\r\n" lines. Keep it parseable.
        // GUI clients commonly parse this.
        let keys = match self.dbsize().await {
            Resp::Integer(n) => n,
            _ => 0,
        };

        let s = format!(
            "# Server\r\nredis_version:0.0.1\r\ncache_service:true\r\n\
             # Stats\r\nkeys:{}\r\nshards:{}\r\neviction:{}\r\n",
            keys, self.cfg.shards, self.cfg.eviction
        );
        Resp::Bulk(Some(Bytes::from(s)))
    }

    pub async fn scan(&self, args: ScanArgs) -> Resp {
        // Global scan by pulling key snapshots from all shards (slow path).
        // Cursor is index into the merged key list.
        let mut all: Vec<Bytes> = Vec::new();
        for i in 0..self.shard_txs.len() {
            if let Resp::Array(keys) = self.call_shard(i, ShardCmd::KeysSnapshot).await {
                for k in keys {
                    if let Resp::Bulk(Some(b)) = k {
                        all.push(b);
                    }
                }
            }
        }

        // Apply MATCH (glob-like: only '*' supported in this milestone)
        if let Some(pat) = &args.match_pat {
            let pat = pat.as_ref();
            all.retain(|k| simple_match(k.as_ref(), pat));
        }

        let count = args.count.unwrap_or(10).max(1);
        let cursor = args.cursor.unwrap_or(0);

        let end = (cursor + count).min(all.len());
        let next = if end >= all.len() { 0 } else { end };

        let batch = all[cursor..end]
            .iter()
            .cloned()
            .map(|b| Resp::Bulk(Some(b)))
            .collect::<Vec<_>>();

        Resp::Array(vec![
            Resp::Bulk(Some(Bytes::from(next.to_string()))),
            Resp::Array(batch),
        ])
    }
}

/* ---------------- SCAN args parsing ---------------- */

#[derive(Debug, Clone)]
pub struct ScanArgs {
    pub cursor: Option<usize>,
    pub match_pat: Option<Bytes>,
    pub count: Option<usize>,
}

impl ScanArgs {
    pub fn from(args: Vec<Bytes>) -> Self {
        // SCAN cursor [MATCH pat] [COUNT n]
        let mut out = ScanArgs { cursor: None, match_pat: None, count: None };
        if !args.is_empty() {
            out.cursor = std::str::from_utf8(args[0].as_ref()).ok().and_then(|s| s.parse().ok());
        }
        let mut i = 1;
        while i + 1 < args.len() {
            if eq_icase(args[i].as_ref(), b"MATCH") {
                out.match_pat = Some(args[i + 1].clone());
                i += 2;
            } else if eq_icase(args[i].as_ref(), b"COUNT") {
                out.count = std::str::from_utf8(args[i + 1].as_ref()).ok().and_then(|s| s.parse().ok());
                i += 2;
            } else {
                i += 1;
            }
        }
        out
    }
}

/* ---------------- Shard actor ---------------- */

enum ShardReq {
    Call { cmd: ShardCmd, resp: oneshot::Sender<Resp> },
    Cleanup,
}

enum ShardCmd {
    Get { key: Bytes },
    Set { key: Bytes, value: Bytes, px_ms: Option<u64> },
    Del { key: Bytes },
    Exists { key: Bytes },
    Expire { key: Bytes, ttl_ms: u64 },
    Ttl { key: Bytes, ms: bool },
    DbSize,
    KeysSnapshot,
}

struct Entry {
    value: Bytes,
    expire_at: Option<Instant>,
}

struct Shard {
    id: usize,
    max_items: usize,
    map: HashMap<Bytes, Entry, ahash::RandomState>,
    evictor: Evictor<Bytes>,
}

impl Shard {
    async fn run(id: usize, max_items: usize, eviction: &str, mut rx: mpsc::Receiver<ShardReq>) {
        let kind = if eviction == "sieve" { EvictionKind::Sieve } else { EvictionKind::Lru };

        info!("shard {} started: max_items={} eviction={}", id, max_items, eviction);

        let mut shard = Shard {
            id,
            max_items,
            map: HashMap::with_hasher(ahash::RandomState::new()),
            evictor: Evictor::new(kind),
        };

        while let Some(req) = rx.recv().await {
            match req {
                ShardReq::Cleanup => shard.cleanup_expired(),
                ShardReq::Call { cmd, resp } => {
                    let out = shard.apply(cmd);
                    let _ = resp.send(out);
                }
            }
        }

        warn!("shard {} stopped", id);
    }

    fn apply(&mut self, cmd: ShardCmd) -> Resp {
        match cmd {
            ShardCmd::Get { key } => self.get(&key),
            ShardCmd::Set { key, value, px_ms } => self.set(key, value, px_ms),
            ShardCmd::Del { key } => Resp::Integer(self.del(&key) as i64),
            ShardCmd::Exists { key } => Resp::Integer(self.exists(&key) as i64),
            ShardCmd::Expire { key, ttl_ms } => Resp::Integer(self.expire(&key, ttl_ms) as i64),
            ShardCmd::Ttl { key, ms } => self.ttl(&key, ms),
            ShardCmd::DbSize => Resp::Integer(self.map.len() as i64),
            ShardCmd::KeysSnapshot => {
                let keys = self.map.keys().cloned().map(|k| Resp::Bulk(Some(k))).collect();
                Resp::Array(keys)
            }
        }
    }

    fn is_expired(e: &Entry) -> bool {
        e.expire_at.map(|t| Instant::now() >= t).unwrap_or(false)
    }

    fn get(&mut self, key: &Bytes) -> Resp {
        if let Some(e) = self.map.get(key) {
            if Self::is_expired(e) {
                // remove expired
                self.map.remove(key);
                self.evictor.on_remove(key);
                return Resp::Bulk(None);
            }
            self.evictor.on_access(key);
            return Resp::Bulk(Some(e.value.clone()));
        }
        Resp::Bulk(None)
    }

    fn set(&mut self, key: Bytes, value: Bytes, px_ms: Option<u64>) -> Resp {
        let expire_at = px_ms.map(|ms| Instant::now() + Duration::from_millis(ms));
        let existed = self.map.contains_key(&key);

        self.map.insert(key.clone(), Entry { value, expire_at });

        // Redis SET clears previous TTL unless specified; we already set expire_at from PX or None.
        if existed {
            self.evictor.on_access(&key);
        } else {
            self.evictor.on_insert(&key);
        }

        self.ensure_capacity();
        Resp::Simple("OK")
    }

    fn del(&mut self, key: &Bytes) -> usize {
        if self.map.remove(key).is_some() {
            self.evictor.on_remove(key);
            1
        } else {
            0
        }
    }

    fn exists(&mut self, key: &Bytes) -> usize {
        match self.map.get(key) {
            Some(e) if !Self::is_expired(e) => 1,
            Some(_) => {
                self.map.remove(key);
                self.evictor.on_remove(key);
                0
            }
            None => 0,
        }
    }

    fn expire(&mut self, key: &Bytes, ttl_ms: u64) -> usize {
        if let Some(e) = self.map.get_mut(key) {
            if Self::is_expired(e) {
                self.map.remove(key);
                self.evictor.on_remove(key);
                return 0;
            }
            e.expire_at = Some(Instant::now() + Duration::from_millis(ttl_ms));
            return 1;
        }
        0
    }

    fn ttl(&mut self, key: &Bytes, ms: bool) -> Resp {
        // Redis semantics:
        // -2 if key does not exist
        // -1 if key exists but has no expire
        if let Some(e) = self.map.get(key) {
            if Self::is_expired(e) {
                self.map.remove(key);
                self.evictor.on_remove(key);
                return Resp::Integer(-2);
            }
            if let Some(exp) = e.expire_at {
                let now = Instant::now();
                let d = if exp > now { exp - now } else { Duration::from_millis(0) };
                let v = if ms { d.as_millis() as i64 } else { d.as_secs() as i64 };
                return Resp::Integer(v);
            }
            return Resp::Integer(-1);
        }
        Resp::Integer(-2)
    }

    fn cleanup_expired(&mut self) {
        let now = Instant::now();
        let mut expired: Vec<Bytes> = Vec::new();
        for (k, v) in self.map.iter() {
            if let Some(t) = v.expire_at {
                if now >= t {
                    expired.push(k.clone());
                }
            }
        }
        if !expired.is_empty() {
            debug!("shard {} expired {} keys", self.id, expired.len());
        }
        for k in expired {
            self.map.remove(&k);
            self.evictor.on_remove(&k);
        }
    }

    fn ensure_capacity(&mut self) {
        while self.map.len() > self.max_items {
            if let Some(victim) = self.evictor.evict() {
                self.map.remove(&victim);
            } else {
                break;
            }
        }
    }
}

/* ---------------- helpers ---------------- */

fn eq_icase(a: &[u8], b: &[u8]) -> bool {
    fn up(c: u8) -> u8 {
        if (b'a'..=b'z').contains(&c) { c - 32 } else { c }
    }
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).all(|(&x, &y)| up(x) == up(y))
}

// Minimal glob match: supports '*' wildcard only (milestone)
fn simple_match(text: &[u8], pat: &[u8]) -> bool {
    if pat == b"*" { return true; }
    // very small matcher: split by '*'
    let parts: Vec<&[u8]> = pat.split(|&c| c == b'*').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() { return true; }

    let mut idx = 0usize;
    for p in parts {
        if let Some(pos) = find_subslice(&text[idx..], p) {
            idx += pos + p.len();
        } else {
            return false;
        }
    }
    true
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
