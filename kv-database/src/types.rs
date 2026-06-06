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

/// 数据库统计信息
#[derive(Debug, Clone)]
pub struct StoreInfo {
    /// 有效键数量（未过期）
    pub key_count: usize,
    /// 总条目数量（含已过期但未清理的）
    pub total_entries: usize,
    /// 最大容量（None 表示无限制）
    pub capacity: Option<usize>,
    /// 持久化文件路径
    pub file_path: String,
}
