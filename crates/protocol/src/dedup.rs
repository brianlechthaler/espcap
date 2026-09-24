use std::collections::{HashMap, VecDeque};

pub const DEDUP_CAP: usize = 256;

/// MAC table that drops the oldest key once `cap` entries are stored.
pub struct MacLru<V> {
    cap: usize,
    map: HashMap<[u8; 6], V>,
    order: VecDeque<[u8; 6]>,
}

impl<V> MacLru<V> {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn contains(&self, key: &[u8; 6]) -> bool {
        self.map.contains_key(key)
    }

    pub fn get_or_insert_with(&mut self, key: [u8; 6], make: impl FnOnce() -> V) -> &mut V {
        if self.map.contains_key(&key) {
            return self.map.get_mut(&key).unwrap();
        }
        if self.map.len() >= self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.order.push_back(key);
        self.map.insert(key, make());
        self.map.get_mut(&key).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_and_updates_existing() {
        let mut t = MacLru::new(2);
        *t.get_or_insert_with([1, 0, 0, 0, 0, 0], || 1) += 1;
        *t.get_or_insert_with([2, 0, 0, 0, 0, 0], || 1) += 1;
        assert_eq!(t.len(), 2);
        *t.get_or_insert_with([3, 0, 0, 0, 0, 0], || 1) += 1;
        assert_eq!(t.len(), 2);
        assert!(!t.contains(&[1, 0, 0, 0, 0, 0]));
        assert!(t.contains(&[2, 0, 0, 0, 0, 0]));
        assert!(t.contains(&[3, 0, 0, 0, 0, 0]));
        assert_eq!(*t.get_or_insert_with([2, 0, 0, 0, 0, 0], || 0), 2);
        let empty = MacLru::<u8>::new(0);
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }
}
