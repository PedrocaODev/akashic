# 19 — $O(\log N)$ Symbol Store LRU Cache Eviction and Min-Heap Adaptive Debouncer

**What to build:** Replace linear-scan eviction in the **Symbol Store LRU Cache** and linear per-tick timer polling in the **Gerrit Event Stream** adaptive debouncer. Maintain a secondary ordered access index (`BTreeSet<(u128, String)>`) in the **Symbol Store LRU Cache** so multi-entry eviction runs in $O(K \log N)$ time while holding the cache mutex, and maintain a deadline min-heap in `AdaptiveDebouncer` so supervisor ticks with no expired batches return in $O(1)$ time.

**Blocked by:** None — can start immediately

**Status:** done

- [x] **Symbol Store LRU Cache** updates a secondary `(last_accessed_ns, key)` ordered index on insert and access, evicting the oldest entries in $O(\log N)$ per eviction instead of scanning all values with `.min_by_key(...)`.
- [x] `AdaptiveDebouncer` tracks pending project deadlines in a min-heap so `pop_ready` inspects the earliest deadline in $O(1)$ time when no batch has expired.
- [x] Existing and new unit tests for **Symbol Store LRU Cache** byte-budget eviction and `AdaptiveDebouncer` quiet-window / max-cap expiration pass.
