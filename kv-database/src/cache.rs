//! 缓存层 - 内存数据管理 + TTL 过期处理 + LRU 淘汰

use crate::error::{Error, Result};
use crate::lru::LruTracker;
use crate::types::{Entry, Store, StoreInfo};
use std::collections::HashMap;

/// 缓存管理器
pub struct Cache {
    /// 存储数据
    store: Store,
    /// 当前时间戳覆盖值（None 表示使用系统实时时间）
    current_time: Option<u64>,
    /// 最大容量（None 表示无限制）
    max_capacity: Option<usize>,
    /// LRU 访问追踪器（追踪写操作顺序）
    lru: LruTracker,
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache {
    /// 创建新的缓存实例
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
            current_time: None,
            max_capacity: None,
            lru: LruTracker::new(),
        }
    }

    /// 获取当前时间（测试模式返回设置值，否则返回实时系统时间）
    pub fn now(&self) -> u64 {
        match self.current_time {
            Some(t) => t,
            None => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// 设置当前时间（用于测试时间控制）
    pub fn set_time(&mut self, time: u64) {
        self.current_time = Some(time);
    }

    /// 获取条目
    pub fn get(&self, key: &str) -> Result<Vec<u8>> {
        let now = self.now();

        match self.store.get(key) {
            Some(entry) => {
                if entry.is_expired(now) {
                    Err(Error::KeyNotFound(key.to_string()))
                } else {
                    Ok(entry.value.clone())
                }
            }
            None => Err(Error::KeyNotFound(key.to_string())),
        }
    }

    /// 存储条目
    pub fn put(&mut self, key: String, value: Vec<u8>, ttl: Option<u64>) {
        let now = self.now();
        let expires_at = ttl.map(|t| now + t);
        let entry = Entry::new(value, now, expires_at);
        self.store.insert(key.clone(), entry);
        self.lru.touch(&key);

        // LRU 淘汰：超出容量时逐出最久未使用的键
        if let Some(cap) = self.max_capacity {
            while self.store.len() > cap {
                if let Some(evicted_key) = self.lru.evict_lru() {
                    self.store.remove(&evicted_key);
                } else {
                    break;
                }
            }
        }
    }

    /// 删除条目
    pub fn delete(&mut self, key: &str) -> Result<()> {
        if self.store.remove(key).is_none() {
            return Err(Error::KeyNotFound(key.to_string()));
        }
        self.lru.remove(key);
        Ok(())
    }

    /// 列出所有未过期的键值对
    pub fn list(&self) -> Vec<(String, Vec<u8>)> {
        let now = self.now();
        self.store
            .iter()
            .filter(|(_, entry)| !entry.is_expired(now))
            .map(|(k, v)| (k.clone(), v.value.clone()))
            .collect()
    }

    /// 设置条目过期
    pub fn expire(&mut self, key: &str, ttl: u64) -> Result<()> {
        let now = self.now();
        match self.store.get_mut(key) {
            Some(entry) => {
                entry.expires_at = Some(now + ttl);
                Ok(())
            }
            None => Err(Error::KeyNotFound(key.to_string())),
        }
    }

    /// 清理过期数据，返回移除数量
    pub fn cleanup(&mut self) -> usize {
        let now = self.now();
        let mut removed = 0;
        self.store.retain(|key, entry| {
            let keep = !entry.is_expired(now);
            if !keep {
                self.lru.remove(key);
                removed += 1;
            }
            keep
        });
        removed
    }

    /// 检查键是否存在且未过期
    pub fn contains_key(&self, key: &str) -> bool {
        let now = self.now();
        match self.store.get(key) {
            Some(entry) => !entry.is_expired(now),
            None => false,
        }
    }

    /// 获取内部存储的引用
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// 获取所有未过期的键名
    pub fn keys(&self) -> Vec<String> {
        let now = self.now();
        self.store
            .iter()
            .filter(|(_, entry)| !entry.is_expired(now))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// 获取有效条目数量（未过期的）
    pub fn len(&self) -> usize {
        let now = self.now();
        self.store
            .values()
            .filter(|entry| !entry.is_expired(now))
            .count()
    }

    /// 是否为空（无有效条目）
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 设置最大容量。如果新容量小于当前条目数，立即淘汰多余条目。
    /// 返回淘汰的条目数量。
    pub fn set_capacity(&mut self, cap: Option<usize>) -> usize {
        self.max_capacity = cap;
        let mut evicted = 0;
        if let Some(max) = self.max_capacity {
            while self.store.len() > max {
                if let Some(evicted_key) = self.lru.evict_lru() {
                    self.store.remove(&evicted_key);
                    evicted += 1;
                } else {
                    break;
                }
            }
        }
        evicted
    }

    /// 获取当前最大容量
    pub fn get_capacity(&self) -> Option<usize> {
        self.max_capacity
    }

    /// 获取存储统计信息
    pub fn info(&self) -> StoreInfo {
        StoreInfo {
            key_count: self.len(),
            total_entries: self.store.len(),
            capacity: self.max_capacity,
            file_path: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_and_get() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.put("key1".to_string(), b"value1".to_vec(), None);

        assert_eq!(cache.get("key1").unwrap(), b"value1");
    }

    #[test]
    fn test_ttl_expiration() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.put("key1".to_string(), b"value1".to_vec(), Some(10));

        // 未过期
        assert!(cache.get("key1").is_ok());

        // 设置时间到过期后（初始时间 + 11秒）
        cache.set_time(1011);

        // 已过期
        assert!(cache.get("key1").is_err());
    }

    #[test]
    fn test_delete() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.put("key1".to_string(), b"value1".to_vec(), None);

        assert!(cache.delete("key1").is_ok());
        assert!(cache.get("key1").is_err());
    }

    #[test]
    fn test_list() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.put("key1".to_string(), b"value1".to_vec(), None);
        cache.put("key2".to_string(), b"value2".to_vec(), Some(10));
        cache.put("key3".to_string(), b"value3".to_vec(), None);

        // 使 key2 过期
        cache.set_time(1011);

        let items = cache.list();
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|(k, _)| k == "key1"));
        assert!(items.iter().any(|(k, _)| k == "key3"));
    }

    #[test]
    fn test_expire() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.put("key1".to_string(), b"value1".to_vec(), None);

        // 设置过期
        cache.expire("key1", 10).unwrap();

        // 未过期
        assert!(cache.get("key1").is_ok());

        // 过期后
        cache.set_time(1011);
        assert!(cache.get("key1").is_err());
    }

    // === LRU 测试 ===

    #[test]
    fn test_lru_eviction_on_put() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.set_capacity(Some(3));

        cache.put("a".to_string(), b"1".to_vec(), None);
        cache.put("b".to_string(), b"2".to_vec(), None);
        cache.put("c".to_string(), b"3".to_vec(), None);
        cache.put("d".to_string(), b"4".to_vec(), None); // 应淘汰 a

        assert!(cache.get("a").is_err());
        assert!(cache.contains_key("b"));
        assert!(cache.contains_key("c"));
        assert!(cache.contains_key("d"));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_lru_write_order() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.set_capacity(Some(2));

        cache.put("a".to_string(), b"1".to_vec(), None);
        cache.put("b".to_string(), b"2".to_vec(), None);
        // 重新写入 a，使其变为最近写入
        cache.put("a".to_string(), b"1_updated".to_vec(), None);
        cache.put("c".to_string(), b"3".to_vec(), None); // 应淘汰 b

        assert!(cache.contains_key("a"));
        assert!(cache.get("b").is_err());
        assert!(cache.contains_key("c"));
        assert_eq!(cache.get("a").unwrap(), b"1_updated");
    }

    #[test]
    fn test_lru_delete_updates_tracker() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.set_capacity(Some(2));

        cache.put("a".to_string(), b"1".to_vec(), None);
        cache.put("b".to_string(), b"2".to_vec(), None);
        cache.delete("a").unwrap();
        cache.put("c".to_string(), b"3".to_vec(), None); // 不应淘汰，还有空间

        assert!(cache.contains_key("b"));
        assert!(cache.contains_key("c"));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_set_capacity_shrink() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.put("a".to_string(), b"1".to_vec(), None);
        cache.put("b".to_string(), b"2".to_vec(), None);
        cache.put("c".to_string(), b"3".to_vec(), None);

        let evicted = cache.set_capacity(Some(1));
        assert_eq!(evicted, 2);
        assert_eq!(cache.len(), 1);
        // c 是最近插入的（最近访问），应保留
        assert!(cache.contains_key("c"));
    }

    #[test]
    fn test_cleanup_returns_count() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.put("a".to_string(), b"1".to_vec(), Some(5));
        cache.put("b".to_string(), b"2".to_vec(), None);
        cache.put("c".to_string(), b"3".to_vec(), Some(5));

        cache.set_time(1010);
        let removed = cache.cleanup();
        assert_eq!(removed, 2);
        assert!(cache.contains_key("b"));
        assert!(!cache.contains_key("a"));
        assert!(!cache.contains_key("c"));
    }

    #[test]
    fn test_keys_method() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.put("z".to_string(), b"1".to_vec(), None);
        cache.put("a".to_string(), b"2".to_vec(), None);
        cache.put("m".to_string(), b"3".to_vec(), None);

        let mut keys = cache.keys();
        keys.sort();
        assert_eq!(keys, vec!["a", "m", "z"]);
    }

    #[test]
    fn test_info() {
        let mut cache = Cache::new();
        cache.set_time(1000);
        cache.set_capacity(Some(100));
        cache.put("k1".to_string(), b"v1".to_vec(), None);
        cache.put("k2".to_string(), b"v2".to_vec(), None);

        let info = cache.info();
        assert_eq!(info.key_count, 2);
        assert_eq!(info.total_entries, 2);
        assert_eq!(info.capacity, Some(100));
    }
}
