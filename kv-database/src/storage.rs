//! 持久化存储层

use crate::error::{Error, Result};
use crate::types::Store;
use bincode::deserialize;
use std::fs::{self, File};
use std::path::Path;

/// 从文件加载存储数据
pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Store> {
    let path = path.as_ref();

    if !path.exists() {
        return Ok(Store::new());
    }

    let data = fs::read(path)?;
    let store: Store = deserialize(&data)
        .map_err(|e| Error::Serialize(format!("failed to deserialize: {}", e)))?;

    Ok(store)
}

/// 将存储数据保存到文件
pub fn save_to_file<P: AsRef<Path>>(path: P, store: &Store) -> Result<()> {
    let data = bincode::serialize(store)
        .map_err(|e| Error::Serialize(format!("failed to serialize: {}", e)))?;

    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(path)?;
    use std::io::Write;
    file.write_all(&data)?;

    Ok(())
}
