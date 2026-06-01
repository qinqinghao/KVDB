//! 类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 数据库中的单个条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// 序列化后的值
    pub value: Vec<u8>,
    /// 创建时间戳（秒）
    pub created_at: u64,
    /// 过期时间戳（秒），None 表示永不过期
    pub expires_at: Option<u64>,
}

impl Entry {
    /// 创建一个新的条目
    pub fn new(value: Vec<u8>, created_at: u64, expires_at: Option<u64>) -> Self {
        Self {
            value,
            created_at,
            expires_at,
        }
    }

    /// 检查条目是否已过期
    pub fn is_expired(&self, now: u64) -> bool {
        match self.expires_at {
            Some(expire_time) => now > expire_time,
            None => false,
        }
    }
}

/// 数据库中的所有条目
pub type Store = HashMap<String, Entry>;
