//! 错误类型定义

use std::fmt;

/// 数据库操作错误
#[derive(Debug)]
pub enum Error {
    /// I/O 错误
    Io(std::io::Error),
    /// 序列化/反序列化错误
    Serialize(String),
    /// 键不存在
    KeyNotFound(String),
    /// 无效路径
    InvalidPath(String),
    /// 服务器错误
    Server(String),
    /// 事务执行失败
    TransactionFailed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Serialize(e) => write!(f, "Serialize error: {}", e),
            Error::KeyNotFound(k) => write!(f, "key not found: {}", k),
            Error::InvalidPath(p) => write!(f, "invalid path: {}", p),
            Error::Server(msg) => write!(f, "server error: {}", msg),
            Error::TransactionFailed(msg) => write!(f, "transaction failed: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// 结果类型别名
pub type Result<T> = std::result::Result<T, Error>;
