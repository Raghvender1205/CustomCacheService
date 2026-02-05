use hashbrown::HashMap;
use slab::Slab;
use std::hash::Hash;

#[derive(Debug, Clone, Copy)]
pub enum EvictionKind {
    Lru,
    Sieve,
}

pub trait EvictionPolicy<K>: Send {
    fn on_insert(&mut self, key: &K);
    fn on_access(&mut self, key: &K);
    fn on_remove(&mut self, key: &K);
    fn evict(&mut self) -> Option<K>;
}

// Public wrapper (so engine/shards don’t need generics everywhere)
pub struct Evictor<K> {
    inner: Box<dyn EvictionPolicy<K>>,
}

impl<K> Evictor<K>
where
    K: Clone + Eq + Hash + Send + 'static,
{
    pub fn new(kind: EvictionKind) -> Self {
        let inner: Box<dyn EvictionPolicy<K>> = match kind {
            EvictionKind::Lru => Box::new(Lru::<K>::new()),
            EvictionKind::Sieve => Box::new(Sieve::<K>::new()),
        };
        Self { inner }
    }

    pub fn on_insert(&mut self, key: &K) {
        self.inner.on_insert(key);
    }
    pub fn on_access(&mut self, key: &K) {
        self.inner.on_access(key);
    }
    pub fn on_remove(&mut self, key: &K) {
        self.inner.on_remove(key);
    }
    pub fn evict(&mut self) -> Option<K> {
        self.inner.evict()
    }
}

/* ---------------- intrusive list core ----------------
Order: head = newest, tail = oldest
prev points toward newer (head)
next points toward older (tail)
*/

struct Node<K> {
    key: K,
    prev: Option<usize>,
    next: Option<usize>,
    visited: bool,
}

struct List<K> {
    head: Option<usize>,
    tail: Option<usize>,
    hand: Option<usize>, // used by SIEVE
    nodes: Slab<Node<K>>,
    index: HashMap<K, usize, ahash::RandomState>,
}

impl<K> List<K>
where
    K: Clone + Eq + Hash,
{
    fn new() -> Self {
        Self {
            head: None,
            tail: None,
            hand: None,
            nodes: Slab::new(),
            index: HashMap::with_hasher(ahash::RandomState::new()),
        }
    }

    fn contains(&self, key: &K) -> bool {
        self.index.contains_key(key)
    }

    fn insert_head(&mut self, key: K, visited: bool) {
        if self.contains(&key) {
            // caller decides whether to move/touch; ignore here
            return;
        }

        let old_head = self.head;
        let idx = self
            .nodes
            .insert(Node { key: key.clone(), prev: None, next: old_head, visited });

        if let Some(h) = old_head {
            self.nodes[h].prev = Some(idx);
        } else {
            self.tail = Some(idx);
        }

        self.head = Some(idx);
        if self.hand.is_none() {
            self.hand = self.tail;
        }

        self.index.insert(key, idx);
    }

    fn remove(&mut self, key: &K) -> bool {
        let Some(&idx) = self.index.get(key) else { return false; };

        let (prev, next) = {
            let n = &self.nodes[idx];
            (n.prev, n.next)
        };

        if let Some(p) = prev {
            self.nodes[p].next = next;
        } else {
            self.head = next;
        }

        if let Some(n) = next {
            self.nodes[n].prev = prev;
        } else {
            self.tail = prev;
        }

        // keep hand valid
        if self.hand == Some(idx) {
            self.hand = prev.or(self.tail);
        }

        self.nodes.remove(idx);
        self.index.remove(key);

        if self.head.is_none() {
            self.hand = None;
        }

        true
    }

    fn move_to_head(&mut self, key: &K) {
        let Some(&idx) = self.index.get(key) else { return; };
        if self.head == Some(idx) {
            return;
        }

        // detach
        let (prev, next) = {
            let n = &self.nodes[idx];
            (n.prev, n.next)
        };

        if let Some(p) = prev {
            self.nodes[p].next = next;
        } else {
            self.head = next;
        }
        if let Some(n) = next {
            self.nodes[n].prev = prev;
        } else {
            self.tail = prev;
        }

        // attach at head
        let old_head = self.head;
        self.nodes[idx].prev = None;
        self.nodes[idx].next = old_head;

        if let Some(h) = old_head {
            self.nodes[h].prev = Some(idx);
        } else {
            self.tail = Some(idx);
        }
        self.head = Some(idx);
    }

    fn pop_tail(&mut self) -> Option<K> {
        let idx = self.tail?;
        let key = self.nodes[idx].key.clone();
        self.remove(&key);
        Some(key)
    }

    // SIEVE eviction scan:
    // start at hand (or tail), walk prev toward head:
    // while visited: visited=false; cur=cur.prev (wrap to tail)
    // evict first unvisited; hand becomes victim.prev (wrap to tail)
    fn sieve_evict(&mut self) -> Option<K> {
        let tail = self.tail?;
        let mut cur = self.hand.unwrap_or(tail);

        loop {
            let visited = self.nodes.get(cur)?.visited;
            if !visited {
                let victim_key = self.nodes[cur].key.clone();
                let next_hand = self.nodes[cur].prev.or(self.tail);

                self.remove(&victim_key);
                self.hand = next_hand;
                return Some(victim_key);
            } else {
                self.nodes[cur].visited = false;
                cur = self.nodes[cur].prev.unwrap_or(tail);
            }
        }
    }

    fn set_visited(&mut self, key: &K, v: bool) {
        if let Some(&idx) = self.index.get(key) {
            self.nodes[idx].visited = v;
        }
    }
}

/* ---------------- LRU ---------------- */

struct Lru<K> {
    list: List<K>,
}

impl<K> Lru<K>
where
    K: Clone + Eq + Hash,
{
    fn new() -> Self {
        Self { list: List::new() }
    }
}

impl<K> EvictionPolicy<K> for Lru<K>
where
    K: Clone + Eq + Hash + Send + 'static,
{
    fn on_insert(&mut self, key: &K) {
        if !self.list.contains(key) {
            self.list.insert_head(key.clone(), false);
        } else {
            self.list.move_to_head(key);
        }
    }

    fn on_access(&mut self, key: &K) {
        self.list.move_to_head(key);
    }

    fn on_remove(&mut self, key: &K) {
        let _ = self.list.remove(key);
    }

    fn evict(&mut self) -> Option<K> {
        self.list.pop_tail()
    }
}

/* ---------------- SIEVE ---------------- */

struct Sieve<K> {
    list: List<K>,
}

impl<K> Sieve<K>
where
    K: Clone + Eq + Hash,
{
    fn new() -> Self {
        Self { list: List::new() }
    }
}

impl<K> EvictionPolicy<K> for Sieve<K>
where
    K: Clone + Eq + Hash + Send + 'static,
{
    fn on_insert(&mut self, key: &K) {
        if !self.list.contains(key) {
            self.list.insert_head(key.clone(), false);
        }
    }

    fn on_access(&mut self, key: &K) {
        // hits only mark visited; no moves (key SIEVE property)
        self.list.set_visited(key, true);
    }

    fn on_remove(&mut self, key: &K) {
        let _ = self.list.remove(key);
    }

    fn evict(&mut self) -> Option<K> {
        self.list.sieve_evict()
    }
}
