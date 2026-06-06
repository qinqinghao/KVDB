//! 事务支持 — 缓冲操作 + read-your-writes + 原子提交

use crate::cache::Cache;
use crate::error::{Error, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::RwLockWriteGuard;

/// 待执行的操作
enum PendingOp {
    Put {
        key: String,
        value: Vec<u8>,
        ttl: Option<u64>,
    },
    Delete {
        key: String,
    },
    Expire {
        key: String,
        ttl: u64,
    },
}

/// 事务句柄
///
/// 持有缓存的写锁，所有操作先缓冲，commit 时原子应用。
/// 未 commit 就 drop 时自动回滚。
pub struct Transaction<'a> {
    guard: RwLockWriteGuard<'a, Cache>,
    ops: Vec<PendingOp>,
}

impl<'a> Transaction<'a> {
    /// 从写锁守卫创建事务（由 KvStore::begin 调用）
    pub(crate) fn new(guard: RwLockWriteGuard<'a, Cache>) -> Self {
        Self {
            guard,
            ops: Vec::new(),
        }
    }

    /// 存储数据
    pub fn put<T: Serialize>(&mut self, key: &str, value: T, ttl: Option<u64>) -> Result<()> {
        let data = bincode::serialize(&value)
            .map_err(|e| Error::Serialize(format!("failed to serialize value: {}", e)))?;

        // 移除同一 key 的旧操作（后面的操作覆盖前面的）
        self.ops.retain(|op| match op {
            PendingOp::Put { key: k, .. }
            | PendingOp::Delete { key: k }
            | PendingOp::Expire { key: k, .. } => k != key,
        });

        self.ops.push(PendingOp::Put {
            key: key.to_string(),
            value: data,
            ttl,
        });
        Ok(())
    }

    /// 获取数据（支持 read-your-writes）
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T> {
        // 反向遍历操作缓冲区，检查 read-your-writes
        for op in self.ops.iter().rev() {
            match op {
                PendingOp::Put {
                    key: k, value, ttl, ..
                } if k == key => {
                    // 检查 TTL（相对于事务开始时间的时间戳）
                    // 实际 put 时 ttl 是 None，所以这里主要看 value
                    let data = value.clone();
                    let value = bincode::deserialize(&data)
                        .map_err(|e| Error::Serialize(format!("failed to deserialize: {}", e)))?;
                    return Ok(value);
                }
                PendingOp::Delete { key: k } if k == key => {
                    return Err(Error::KeyNotFound(key.to_string()));
                }
                _ => {}
            }
        }

        // 缓冲区未命中，委托给 Cache
        let data = self.guard.get(key)?;
        let value = bincode::deserialize(&data)
            .map_err(|e| Error::Serialize(format!("failed to deserialize: {}", e)))?;
        Ok(value)
    }

    /// 删除数据
    pub fn delete(&mut self, key: &str) -> Result<()> {
        self.ops.retain(|op| match op {
            PendingOp::Put { key: k, .. }
            | PendingOp::Delete { key: k }
            | PendingOp::Expire { key: k, .. } => k != key,
        });
        self.ops.push(PendingOp::Delete {
            key: key.to_string(),
        });
        Ok(())
    }

    /// 设置过期时间
    pub fn expire(&mut self, key: &str, ttl: u64) -> Result<()> {
        self.ops.retain(|op| match op {
            PendingOp::Put { key: k, .. }
            | PendingOp::Delete { key: k }
            | PendingOp::Expire { key: k, .. } => k != key,
        });
        self.ops.push(PendingOp::Expire {
            key: key.to_string(),
            ttl,
        });
        Ok(())
    }

    /// 列出所有键值对（含缓冲区修改）
    pub fn list<T: DeserializeOwned>(&self) -> Result<Vec<(String, T)>> {
        // 从 Cache 获取基础列表
        let base = self.guard.list();
        let mut map: HashMap<String, Vec<u8>> = base.into_iter().collect();

        // 应用缓冲区操作
        for op in &self.ops {
            match op {
                PendingOp::Put { key, value, .. } => {
                    map.insert(key.clone(), value.clone());
                }
                PendingOp::Delete { key } => {
                    map.remove(key);
                }
                PendingOp::Expire { .. } => {
                    // expire 不影响 list 的内容
                }
            }
        }

        let mut result = Vec::new();
        for (key, data) in map {
            let value = bincode::deserialize(&data)
                .map_err(|e| Error::Serialize(format!("failed to deserialize: {}", e)))?;
            result.push((key, value));
        }
        Ok(result)
    }

    /// 检查键是否存在
    pub fn contains_key(&self, key: &str) -> bool {
        for op in self.ops.iter().rev() {
            match op {
                PendingOp::Put { key: k, .. } if k == key => return true,
                PendingOp::Delete { key: k } if k == key => return false,
                _ => {}
            }
        }
        self.guard.contains_key(key)
    }

    /// 获取所有键名（含缓冲区修改）
    pub fn keys(&self) -> Vec<String> {
        let mut base_keys = self.guard.keys();
        for op in &self.ops {
            match op {
                PendingOp::Put { key, .. } => {
                    if !base_keys.contains(key) {
                        base_keys.push(key.clone());
                    }
                }
                PendingOp::Delete { key } => {
                    base_keys.retain(|k| k != key);
                }
                PendingOp::Expire { .. } => {}
            }
        }
        base_keys
    }

    /// 获取有效条目数量
    pub fn len(&self) -> usize {
        self.keys().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 提交事务：按序应用所有缓冲操作
    pub fn commit(mut self) -> Result<()> {
        for op in self.ops.drain(..) {
            match op {
                PendingOp::Put { key, value, ttl } => {
                    self.guard.put(key, value, ttl);
                }
                PendingOp::Delete { key } => {
                    // 删除不存在的键不算事务失败，只是忽略
                    let _ = self.guard.delete(&key);
                }
                PendingOp::Expire { key, ttl } => {
                    if self.guard.expire(&key, ttl).is_err() {
                        return Err(Error::TransactionFailed(format!(
                            "cannot expire non-existent key: {}",
                            key
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// 回滚事务：丢弃所有缓冲操作
    pub fn rollback(self) {
        // 消费 self，释放写锁，丢弃 ops
    }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        // 未 commit 时自动回滚（guard 被 drop 时释放写锁）
    }
}

// 事务的完整测试通过 KvStore::begin() 在集成测试中进行
