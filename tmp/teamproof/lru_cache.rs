use std::collections::{HashMap, VecDeque};

pub struct Lru {
    capacity: usize,
    map: HashMap<i32, i32>,
    order: VecDeque<i32>,
}

impl Lru {
    pub fn new(capacity: usize) -> Self {
        Lru {
            capacity,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&mut self, key: i32) -> Option<i32> {
        if let Some(value) = self.map.get(&key).cloned() {
            self.order.retain(|&k| k != key);
            self.order.push_back(key);
            Some(value)
        } else {
            None
        }
    }

    pub fn put(&mut self, key: i32, value: i32) {
        if let Some(index) = self.order.iter().position(|&k| k == key) {
            self.order.remove(index);
        } else if self.map.len() >= self.capacity {
            if let Some(lru_key) = self.order.pop_front() {
                self.map.remove(&lru_key);
            }
        }
        self.order.push_back(key);
        self.map.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_cache() {
        let mut cache = Lru::new(2);
        cache.put(1, 10);
        cache.put(2, 20);
        assert_eq!(cache.get(1), Some(10));
        cache.put(3, 30);
        assert_eq!(cache.get(2), None);
        assert_eq!(cache.get(3), Some(30));
    }
}