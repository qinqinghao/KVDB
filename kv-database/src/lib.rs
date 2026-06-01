//! 本地键值对数据库
//!
//! 这是一个功能完整的本地键值对数据库，特点包括：
//!
//! - **内存存储**：基于 HashMap 的高效内存存储
//! - **文件持久化**：使用 bincode 进行二进制序列化保存
//! - **TTL 过期**：支持为键设置过期时间
//! - **多线程安全**：使用 RwLock 保证并发访问安全
//! - **自动清理**：提供清理过期数据的功能
//!
//! # 示例
//!
//! ```
//! use kv_database::KvStore;
//!
//! let db = KvStore::new("data.bin").unwrap();
//!
//! // 存储字符串
//! db.put("user:1", "Alice", None).unwrap();
//!
//! // 读取数据
//! let user: String = db.get("user:1").unwrap();
//! assert_eq!(user, "Alice");
//!
//! // 带过期时间存储
//! db.put("session:1", "token123", Some(3600)).unwrap();
//!
//! // 列出所有键值对
//! let items: Vec<(String, String)> = db.list().unwrap();
//! assert_eq!(items.len(), 2);
//! ```

pub mod cache;
pub mod db;
pub mod error;
pub mod storage;
pub mod types;

pub use crate::db::KvStore;
pub use crate::error::{Error, Result};
