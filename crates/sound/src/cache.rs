//! A least-recently-used cache bounded by *bytes*, not by entry count.
//!
//! The spec is explicit that a 5,000-sound library must never be loaded into RAM. A
//! count-based cache cannot express that: 200 short door creaks and 200 two-minute
//! ambience beds are wildly different amounts of memory. So the budget is in bytes and
//! entries are evicted until the total fits.
//!
//! Generic over the value type so it can be tested without an audio device.

use std::hash::Hash;

use lru::LruCache;

pub struct ByteBudgetCache<K: Hash + Eq, V> {
    entries: LruCache<K, (V, usize)>,
    budget_bytes: usize,
    used_bytes: usize,
    size_of: fn(&V) -> usize,
}

// Part of the cache's contract and covered by its tests; `clear` and `set_budget` are
// wired up when the cache budget becomes a user setting.
#[allow(dead_code)]
impl<K: Hash + Eq + Clone, V> ByteBudgetCache<K, V> {
    pub fn new(budget_bytes: usize, size_of: fn(&V) -> usize) -> Self {
        Self {
            entries: LruCache::unbounded(),
            budget_bytes,
            used_bytes: 0,
            size_of,
        }
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    pub fn budget_bytes(&self) -> usize {
        self.budget_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Fetch a cached value, marking it as most recently used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|(value, _)| value)
    }

    /// Insert a value, evicting least-recently-used entries until the budget fits.
    ///
    /// A single value larger than the whole budget is still stored — refusing to cache
    /// it would mean re-decoding it on every play, which is worse than briefly
    /// exceeding the budget. It is evicted as soon as anything else is inserted.
    pub fn insert(&mut self, key: K, value: V) {
        let size = (self.size_of)(&value);

        if let Some((_, old_size)) = self.entries.pop(&key) {
            self.used_bytes = self.used_bytes.saturating_sub(old_size);
        }

        self.entries.put(key, (value, size));
        self.used_bytes += size;
        self.evict_until_within_budget();
    }

    fn evict_until_within_budget(&mut self) {
        while self.used_bytes > self.budget_bytes && self.entries.len() > 1 {
            match self.entries.pop_lru() {
                Some((_, (_, size))) => {
                    self.used_bytes = self.used_bytes.saturating_sub(size);
                }
                None => break,
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.used_bytes = 0;
    }

    /// Drop one entry, e.g. when its file changed on disk.
    pub fn invalidate(&mut self, key: &K) {
        if let Some((_, size)) = self.entries.pop(key) {
            self.used_bytes = self.used_bytes.saturating_sub(size);
        }
    }

    /// Resize the budget, evicting immediately if the new budget is smaller.
    pub fn set_budget(&mut self, budget_bytes: usize) {
        self.budget_bytes = budget_bytes.max(1);
        self.evict_until_within_budget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values are byte vectors so "size" is unambiguous in the tests.
    fn cache(budget: usize) -> ByteBudgetCache<String, Vec<u8>> {
        ByteBudgetCache::new(budget, |v| v.len())
    }

    fn blob(n: usize) -> Vec<u8> {
        vec![0u8; n]
    }

    #[test]
    fn a_stored_value_can_be_read_back() {
        let mut cache = cache(1000);
        cache.insert("a".into(), blob(10));
        assert_eq!(cache.get(&"a".to_string()).map(Vec::len), Some(10));
        assert_eq!(cache.used_bytes(), 10);
    }

    #[test]
    fn insertions_beyond_the_budget_evict_the_least_recently_used() {
        let mut cache = cache(100);
        cache.insert("a".into(), blob(40));
        cache.insert("b".into(), blob(40));

        // Touching "a" makes "b" the least recently used.
        assert!(cache.get(&"a".to_string()).is_some());
        cache.insert("c".into(), blob(40));

        assert!(
            cache.get(&"b".to_string()).is_none(),
            "b should have been evicted"
        );
        assert!(cache.get(&"a".to_string()).is_some());
        assert!(cache.get(&"c".to_string()).is_some());
        assert!(cache.used_bytes() <= cache.budget_bytes());
    }

    #[test]
    fn reinserting_a_key_does_not_double_count_its_bytes() {
        let mut cache = cache(1000);
        cache.insert("a".into(), blob(10));
        cache.insert("a".into(), blob(30));

        assert_eq!(cache.len(), 1);
        assert_eq!(cache.used_bytes(), 30);
    }

    #[test]
    fn an_oversized_value_is_kept_rather_than_dropped() {
        let mut cache = cache(50);
        cache.insert("huge".into(), blob(500));

        assert!(cache.get(&"huge".to_string()).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn an_oversized_value_is_evicted_by_the_next_insert() {
        let mut cache = cache(50);
        cache.insert("huge".into(), blob(500));
        cache.insert("small".into(), blob(10));

        assert!(cache.get(&"huge".to_string()).is_none());
        assert_eq!(cache.used_bytes(), 10);
    }

    #[test]
    fn shrinking_the_budget_evicts_immediately() {
        let mut cache = cache(1000);
        cache.insert("a".into(), blob(400));
        cache.insert("b".into(), blob(400));

        cache.set_budget(500);
        assert!(cache.used_bytes() <= 500);
        assert!(
            cache.get(&"b".to_string()).is_some(),
            "most recent survives"
        );
    }

    #[test]
    fn invalidate_and_clear_free_their_bytes() {
        let mut cache = cache(1000);
        cache.insert("a".into(), blob(10));
        cache.insert("b".into(), blob(20));

        cache.invalidate(&"a".to_string());
        assert_eq!(cache.used_bytes(), 20);

        cache.clear();
        assert_eq!(cache.used_bytes(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn a_long_run_of_inserts_never_exceeds_the_budget() {
        // Stands in for a long session: thousands of one-shots, bounded memory.
        let mut cache = cache(10_000);
        for i in 0..5_000 {
            cache.insert(format!("sound-{i}"), blob(64));
            assert!(cache.used_bytes() <= cache.budget_bytes());
        }
    }
}
