//! 数据库核心实现

use crate::cache::Cache;
use crate::error::{Error, Result};
use crate::storage::{load_from_file, save_to_file};
use crate::transaction::Transaction;
use crate::types::StoreInfo;
use serde::de::DeserializeOwned;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// 数据库存储结构
pub struct KvStore {
    /// 缓存层，使用 RwLock 保证线程安全
    cache: Arc<RwLock<Cache>>,
    /// 持久化文件路径
    path: String,
}

impl KvStore {
    /// 创建新的数据库实例（不恢复数据）
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::InvalidPath("path is not valid unicode".to_string()))?
            .to_string();

        Ok(Self {
            cache: Arc::new(RwLock::new(Cache::new())),
            path: path_str,
        })
    }

    /// 打开数据库（从文件恢复数据）
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::InvalidPath("path is not valid unicode".to_string()))?
            .to_string();

        let store = load_from_file(&path)?;
        let mut cache = Cache::new();
        // 恢复数据时，清理已过期的条目
        let now = cache.now();
        for (key, entry) in store {
            if !entry.is_expired(now) {
                cache.put(key, entry.value, entry.expires_at.map(|t| t - now));
            }
        }

        Ok(Self {
            cache: Arc::new(RwLock::new(cache)),
            path: path_str,
        })
    }

    /// 存储数据
    /// - `key`: 键
    /// - `value`: 值（实现 Serialize 的任意类型）
    /// - `ttl`: 过期时间（秒），None 表示永不过期
    pub fn put<T: serde::Serialize>(&self, key: &str, value: T, ttl: Option<u64>) -> Result<()> {
        let data = bincode::serialize(&value)
            .map_err(|e| Error::Serialize(format!("failed to serialize value: {}", e)))?;

        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire write lock")))?;

        cache.put(key.to_string(), data, ttl);
        Ok(())
    }

    /// 获取数据
    /// - `key`: 键
    /// - 返回: 实现 Deserialize 的值
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire read lock")))?;

        let data = cache.get(key)?;
        let value = bincode::deserialize(&data)
            .map_err(|e| Error::Serialize(format!("failed to deserialize: {}", e)))?;

        Ok(value)
    }

    /// 删除数据
    pub fn delete(&self, key: &str) -> Result<()> {
        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire write lock")))?;

        cache.delete(key)
    }

    /// 列出所有键值对
    pub fn list<T: DeserializeOwned>(&self) -> Result<Vec<(String, T)>> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire read lock")))?;

        let items = cache.list();
        let mut result = Vec::new();

        for (key, data) in items {
            let value = bincode::deserialize(&data)
                .map_err(|e| Error::Serialize(format!("failed to deserialize: {}", e)))?;
            result.push((key, value));
        }

        Ok(result)
    }

    /// 手动保存到文件
    pub fn save(&self) -> Result<()> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire read lock")))?;

        save_to_file(&self.path, cache.store())
    }

    /// 设置条目过期
    pub fn expire(&self, key: &str, ttl: u64) -> Result<()> {
        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire write lock")))?;

        cache.expire(key, ttl)
    }

    /// 检查键是否存在
    pub fn contains_key(&self, key: &str) -> bool {
        let cache = self.cache.read().unwrap();
        cache.contains_key(key)
    }

    /// 清理过期数据，返回移除数量
    pub fn cleanup(&self) -> Result<usize> {
        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire write lock")))?;

        Ok(cache.cleanup())
    }

    /// 开始事务
    ///
    /// 获取写锁并返回事务句柄。事务内所有操作先缓冲，
    /// commit 时原子应用。未 commit 就 drop 时自动回滚。
    pub fn begin(&self) -> Transaction<'_> {
        let guard = self.cache.write().expect("lock poisoned");
        Transaction::new(guard)
    }

    /// 设置最大容量。超出容量时按 LRU 淘汰。
    /// 返回淘汰的条目数量。
    pub fn set_capacity(&self, capacity: Option<usize>) -> Result<usize> {
        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire write lock")))?;

        Ok(cache.set_capacity(capacity))
    }

    /// 获取当前最大容量
    pub fn get_capacity(&self) -> Result<Option<usize>> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire read lock")))?;

        Ok(cache.get_capacity())
    }

    /// 获取所有未过期的键名
    pub fn keys(&self) -> Result<Vec<String>> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire read lock")))?;

        Ok(cache.keys())
    }

    /// 获取有效条目数量
    pub fn len(&self) -> Result<usize> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire read lock")))?;

        Ok(cache.len())
    }

    /// 是否为空（无有效条目）
    pub fn is_empty(&self) -> Result<bool> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire read lock")))?;

        Ok(cache.is_empty())
    }

    /// 获取数据库统计信息
    pub fn info(&self) -> Result<StoreInfo> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Io(std::io::Error::other("failed to acquire read lock")))?;

        let mut info = cache.info();
        info.file_path = self.path.clone();
        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_put_and_get() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        db.put("user:1", "Alice", None).unwrap();

        let value: String = db.get("user:1").unwrap();
        assert_eq!(value, "Alice");
    }

    #[test]
    fn test_persistence() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        // 写入数据
        {
            let db = KvStore::new(&path).unwrap();
            db.put("user:1", "Alice", None).unwrap();
            db.put("user:2", "Bob", None).unwrap();
            db.save().unwrap();
        }

        // 重新打开并读取
        let db = KvStore::open(&path).unwrap();
        let value: String = db.get("user:1").unwrap();
        assert_eq!(value, "Alice");

        let value: String = db.get("user:2").unwrap();
        assert_eq!(value, "Bob");
    }

    #[test]
    fn test_ttl() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        db.put("session:1", "token123", Some(10)).unwrap();

        let value: String = db.get("session:1").unwrap();
        assert_eq!(value, "token123");

        // 手动设置过期
        db.expire("session:1", 5).unwrap();

        // 立即检查（应该还在）
        let value: String = db.get("session:1").unwrap();
        assert_eq!(value, "token123");
    }

    #[test]
    fn test_delete() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        db.put("key", "value", None).unwrap();
        assert!(db.contains_key("key"));

        db.delete("key").unwrap();
        assert!(!db.contains_key("key"));
    }

    #[test]
    fn test_list() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        db.put("a", 1, None).unwrap();
        db.put("b", 2, None).unwrap();
        db.put("c", 3, None).unwrap();

        let items = db.list::<i32>().unwrap();
        assert_eq!(items.len(), 3);
        assert!(items.iter().any(|(k, v)| k == "a" && *v == 1));
        assert!(items.iter().any(|(k, v)| k == "b" && *v == 2));
        assert!(items.iter().any(|(k, v)| k == "c" && *v == 3));
    }

    #[test]
    fn test_concurrent_access() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = Arc::new(KvStore::new(&path).unwrap());

        let mut handles = vec![];
        for i in 0..10 {
            let db_clone = Arc::clone(&db);
            let handle = std::thread::spawn(move || {
                db_clone.put(&format!("key{}", i), i, None).unwrap();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 使用Arc继续使用db，无需unwrap
        for i in 0..10 {
            let value: i32 = db.get(&format!("key{}", i)).unwrap();
            assert_eq!(value, i);
        }
    }

    #[test]
    fn test_keys_and_len() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        db.put("a", 1, None).unwrap();
        db.put("b", 2, None).unwrap();
        db.put("c", 3, None).unwrap();

        assert_eq!(db.len().unwrap(), 3);
        let mut keys = db.keys().unwrap();
        keys.sort();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_info() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        db.put("k", "v", None).unwrap();

        let info = db.info().unwrap();
        assert_eq!(info.key_count, 1);
        assert_eq!(info.capacity, None);
        assert!(info.file_path.contains("db.bin"));
    }

    #[test]
    fn test_lru_capacity() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        db.set_capacity(Some(2)).unwrap();

        db.put("a", 1, None).unwrap();
        db.put("b", 2, None).unwrap();
        db.put("c", 3, None).unwrap(); // 淘汰 a

        assert!(db.get::<i32>("a").is_err());
        assert_eq!(db.get::<i32>("b").unwrap(), 2);
        assert_eq!(db.get::<i32>("c").unwrap(), 3);
        assert_eq!(db.len().unwrap(), 2);
    }

    #[test]
    fn test_transaction_commit() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        {
            let mut txn = db.begin();
            txn.put("tx_key", "tx_value", None).unwrap();
            txn.put("tx_num", 42, None).unwrap();
            txn.commit().unwrap();
        }

        assert_eq!(db.get::<String>("tx_key").unwrap(), "tx_value");
        assert_eq!(db.get::<i32>("tx_num").unwrap(), 42);
    }

    #[test]
    fn test_transaction_rollback() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        db.put("existing", "old_value", None).unwrap();

        {
            let mut txn = db.begin();
            txn.put("existing", "new_value", None).unwrap();
            txn.put("temp_key", "temp", None).unwrap();
            txn.rollback();
        }

        assert_eq!(db.get::<String>("existing").unwrap(), "old_value");
        assert!(db.get::<String>("temp_key").is_err());
    }

    #[test]
    fn test_transaction_read_your_writes() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        {
            let mut txn = db.begin();
            txn.put("k", "hello", None).unwrap();
            // 在事务内应能读到
            let val: String = txn.get("k").unwrap();
            assert_eq!(val, "hello");
            txn.commit().unwrap();
        }
    }

    #[test]
    fn test_transaction_drop_rollback() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        {
            let mut txn = db.begin();
            txn.put("will_disappear", "gone", None).unwrap();
            // 不 commit，drop 时自动回滚
        }

        assert!(db.get::<String>("will_disappear").is_err());
    }

    #[test]
    fn test_cleanup_returns_count() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("db.bin");

        let db = KvStore::new(&path).unwrap();
        db.put("k1", "v1", None).unwrap();
        db.put("k2", "v2", Some(1)).unwrap();

        // 等待过期
        std::thread::sleep(std::time::Duration::from_secs(2));

        let removed = db.cleanup().unwrap();
        assert_eq!(removed, 1);
        assert!(db.contains_key("k1"));
        assert!(!db.contains_key("k2"));
    }
}
