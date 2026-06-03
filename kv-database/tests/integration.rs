//! 集成测试

use kv_database::KvStore;

#[test]
fn test_full_workflow() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("test.db");

    // 创建并写入
    let db = KvStore::new(&path).unwrap();

    db.put("name", "Test User", None).unwrap();
    db.put("age", 30, None).unwrap();
    db.put("active", true, None).unwrap();

    db.save().unwrap();

    // 读取验证
    let name: String = db.get("name").unwrap();
    let age: u32 = db.get("age").unwrap();
    let active: bool = db.get("active").unwrap();

    assert_eq!(name, "Test User");
    assert_eq!(age, 30);
    assert!(active);

    // 更新
    db.put("age", 31, None).unwrap();
    let age: u32 = db.get("age").unwrap();
    assert_eq!(age, 31);

    // 删除
    db.delete("active").unwrap();
    assert!(!db.contains_key("active"));

    // 列出 - 注意：list 要求所有值是相同类型
    // 这里只存 String 类型来测试 list 功能
    let db2 = KvStore::new(&path).unwrap();
    db2.put("key1", "value1", None).unwrap();
    db2.put("key2", "value2", None).unwrap();
    let items: Vec<(String, String)> = db2.list().unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn test_persistence_across_openings() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("persistent.db");

    let path_clone = path.clone();

    // 第一次写入
    {
        let db = KvStore::new(&path).unwrap();
        db.put("key1", "value1", None).unwrap();
        db.put("key2", "value2", None).unwrap();
        db.save().unwrap();
    }

    // 重新打开
    {
        let db = KvStore::open(&path_clone).unwrap();

        let v1: String = db.get("key1").unwrap();
        let v2: String = db.get("key2").unwrap();

        assert_eq!(v1, "value1");
        assert_eq!(v2, "value2");
    }
}

#[test]
fn test_error_handling() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("error_test.db");

    let db = KvStore::new(&path).unwrap();

    // 获取不存在的键
    let result: Result<String, kv_database::Error> = db.get("nonexistent");
    assert!(result.is_err());

    // 删除不存在的键
    let result = db.delete("nonexistent");
    assert!(result.is_err());
}
