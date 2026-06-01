//! 缓存层 - 内存数据管理 + TTL 过期处理

use crate::error::{Error, Result};
use crate::types::{Entry, Store};

/// 缓存管理器
pub struct Cache {
    /// 存储数据
    store: Store,
    /// 当前时间戳（秒），用于测试时的时间控制
    current_time: u64,
}

impl Cache {
    /// 创建新的缓存实例
    pub fn new() -> Self {
        Self {
            store: Store::new(),
            current_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// 获取当前时间
    pub fn now(&self) -> u64 {
        self.current_time
    }

    /// 设置当前时间（主要用于测试）
    pub fn set_time(&mut self, time: u64) {
        self.current_time = time;
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
        self.store.insert(key, entry);
    }

    /// 删除条目
    pub fn delete(&mut self, key: &str) -> Result<()> {
        if self.store.remove(key).is_none() {
            return Err(Error::KeyNotFound(key.to_string()));
        }
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

    /// 清理过期数据
    pub fn cleanup(&mut self) {
        let now = self.now();
        self.store.retain(|_, entry| !entry.is_expired(now));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_and_get() {
        let mut cache = Cache::new();
        cache.put("key1".to_string(), b"value1".to_vec(), None);

        assert_eq!(cache.get("key1").unwrap(), b"value1");
    }

    #[test]
    fn test_ttl_expiration() {
        let mut cache = Cache::new();
        let initial_time = cache.now();
        cache.put("key1".to_string(), b"value1".to_vec(), Some(10));

        // 未过期
        assert!(cache.get("key1").is_ok());

        // 设置时间到过期后（初始时间 + 11秒）
        cache.set_time(initial_time + 11);

        // 已过期
        assert!(cache.get("key1").is_err());
    }

    #[test]
    fn test_delete() {
        let mut cache = Cache::new();
        cache.put("key1".to_string(), b"value1".to_vec(), None);

        assert!(cache.delete("key1").is_ok());
        assert!(cache.get("key1").is_err());
    }

    #[test]
    fn test_list() {
        let mut cache = Cache::new();
        let initial_time = cache.now();
        cache.put("key1".to_string(), b"value1".to_vec(), None);
        cache.put("key2".to_string(), b"value2".to_vec(), Some(10));
        cache.put("key3".to_string(), b"value3".to_vec(), None);

        // 使 key2 过期
        cache.set_time(initial_time + 11);

        let items = cache.list();
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|(k, _)| k == "key1"));
        assert!(items.iter().any(|(k, _)| k == "key3"));
    }

    #[test]
    fn test_expire() {
        let mut cache = Cache::new();
        let initial_time = cache.now();
        cache.put("key1".to_string(), b"value1".to_vec(), None);

        // 设置过期
        cache.expire("key1", 10).unwrap();

        // 未过期
        assert!(cache.get("key1").is_ok());

        // 过期后
        cache.set_time(initial_time + 11);
        assert!(cache.get("key1").is_err());
    }
}
