# KV Database — 基于 Rust 的本地键值数据库

一个功能完整的本地键值存储数据库，使用纯安全 Rust 编写。支持内存缓存、TTL 过期、LRU 淘汰、事务、TCP 网络服务、CLI 命令行工具和 REPL 交互终端。

## 特性

- **内存缓存**：基于 HashMap 的高效内存存储，支持存储任意可序列化的 Rust 类型
- **TTL 过期**：支持为键设置过期时间（秒级），自动过滤已过期数据
- **LRU 淘汰**：基于 arena 双向链表的 O(1) LRU 淘汰策略，支持容量限制
- **事务支持**：BEGIN/COMMIT/ROLLBACK 交互式事务，支持 Read-Your-Writes 语义
- **文件持久化**：bincode 二进制序列化，启动时自动恢复数据
- **TCP 网络服务**：文本协议，多线程处理，类 Redis 命令格式
- **CLI 工具**：12 个子命令，支持单次执行和 REPL 交互模式
- **线程安全**：Arc + RwLock 保证多线程并发访问安全
- **纯安全 Rust**：零 unsafe 代码

## 快速开始

### 安装依赖

```bash
# 确保已安装 Rust 工具链
rustc --version  # >= 1.70

# 克隆项目
git clone <repo-url>
cd kv-database
```

### CLI 单次命令

```bash
# 存储数据
cargo run -- set hello "Hello, Rust!"

# 读取数据
cargo run -- get hello

# 列出所有键值对
cargo run -- list

# 查看所有键名
cargo run -- keys

# 查看数据库信息
cargo run -- info

# 设置过期时间（秒）
cargo run -- expire hello 60

# 删除数据
cargo run -- del hello

# 清理过期数据
cargo run -- cleanup

# 手动保存
cargo run -- save

# 查看容量
cargo run -- capacity
```

### REPL 交互模式

```bash
cargo run
```

进入 REPL 后支持的命令：

```
Commands:
  SET <key> <value> [<ttl>]    存储数据（可选 TTL）
  GET <key>                    获取数据
  DEL <key>                    删除数据
  LIST                         列出所有键值对
  KEYS                         列出所有键名
  EXPIRE <key> <ttl>          设置过期时间（秒）
  INFO                         查看数据库统计信息
  CLEANUP                      清理过期数据
  SAVE                         手动保存到磁盘
  CAPACITY [<n>]               查看/设置最大容量（0 = 无限制）
  BEGIN                        开始事务
  COMMIT                       提交事务
  ROLLBACK                     回滚事务
  HELP                         显示帮助
  QUIT                         退出（自动保存）
```

### 事务操作（REPL 内）

```
kv> SET balance 1000
kv> BEGIN
txn> SET balance 500
txn> GET balance
"500"
txn> COMMIT
kv> GET balance
"500"
```

回滚示例：
```
kv> BEGIN
txn> SET temp "this will disappear"
txn> ROLLBACK
kv> GET temp
(nil)
```

### LRU 容量管理（REPL 内）

```
kv> CAPACITY 3
Capacity set to 3 (0 evicted)
kv> SET a 1
kv> SET b 2
kv> SET c 3
kv> SET d 4
kv> KEYS
  b
  c
  d
kv> GET a
(nil)
```

### TCP 服务器

```bash
# 启动服务器
cargo run -- serve --port 6379
```

客户端连接：

```bash
telnet localhost 6379
```

支持的命令：

| 命令 | 格式 | 说明 |
|------|------|------|
| SET | `SET key value [ttl]` | 存储数据 |
| GET | `GET key` | 获取数据 |
| DEL | `DEL key` | 删除数据 |
| LIST | `LIST` | 列出所有键值对 |
| KEYS | `KEYS` | 列出所有键名 |
| EXPIRE | `EXPIRE key ttl` | 设置过期 |
| INFO | `INFO` | 数据库信息 |
| CLEANUP | `CLEANUP` | 清理过期数据 |
| QUIT | `QUIT` | 断开连接 |

## 作为 Rust 库使用

在 `Cargo.toml` 中添加：

```toml
[dependencies]
kv-database = "0.1.0"
```

### 基本使用

```rust
use kv_database::KvStore;

let db = KvStore::new("data.bin").unwrap();

// 存储数据
db.put("user:1", "Alice", None).unwrap();
db.put("count", 42, None).unwrap();

// 读取数据
let user: String = db.get("user:1").unwrap();
let count: i32 = db.get("count").unwrap();

// 带 TTL 存储（3600 秒后过期）
db.put("session:1", "token123", Some(3600)).unwrap();

// 检查键是否存在
if db.contains_key("user:1") {
    println!("User exists");
}

// 列出所有数据
let items: Vec<(String, String)> = db.list().unwrap();

// 删除键
db.delete("user:1").unwrap();

// 手动持久化
db.save().unwrap();
```

### 事务

```rust
let db = KvStore::new("data.bin").unwrap();

// 提交事务
{
    let mut txn = db.begin();
    txn.put("key1", "value1", None).unwrap();
    txn.put("key2", 42, None).unwrap();
    txn.commit().unwrap();
}

// 回滚事务
{
    let mut txn = db.begin();
    txn.put("temp", "will be discarded", None).unwrap();
    // txn 离开作用域，自动回滚
}

let result = db.get::<String>("temp");
assert!(result.is_err());
```

### LRU 容量管理

```rust
let db = KvStore::new("data.bin").unwrap();

// 设置最大容量为 2
db.set_capacity(Some(2)).unwrap();

db.put("a", 1, None).unwrap();
db.put("b", 2, None).unwrap();
db.put("c", 3, None).unwrap(); // a 被淘汰

assert!(db.get::<i32>("a").is_err());
assert_eq!(db.get::<i32>("b").unwrap(), 2);
assert_eq!(db.get::<i32>("c").unwrap(), 3);
```

### 数据库信息

```rust
let db = KvStore::new("data.bin").unwrap();
db.put("k1", "v1", None).unwrap();
db.put("k2", "v2", None).unwrap();

let info = db.info().unwrap();
println!("Keys: {}", info.key_count);        // 2
println!("Capacity: {:?}", info.capacity);    // None
println!("File: {}", info.file_path);         // data.bin
```

## 项目结构

```
kv-database/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs             # CLI 二进制入口：clap 子命令 + REPL 模式
│   ├── lib.rs              # 库入口：模块声明与重导出
│   ├── cache.rs            # 缓存层：内存数据管理、TTL 过期、LRU 集成
│   ├── db.rs               # 数据库核心：统一对外 API、事务入口
│   ├── error.rs            # 错误类型：自定义 Error 枚举
│   ├── lru.rs              # LRU 追踪器：arena 双向链表，O(1) 操作
│   ├── server.rs           # TCP 服务器：文本协议、多线程处理
│   ├── storage.rs          # 文件持久化：二进制序列化
│   ├── transaction.rs      # 事务支持：操作缓冲 + Read-Your-Writes
│   └── types.rs            # 数据类型：Entry、Store、StoreInfo
└── tests/
    └── integration.rs      # 集成测试
```

## API 总览

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `(path) -> Result<KvStore>` | 创建新数据库 |
| `open` | `(path) -> Result<KvStore>` | 从文件恢复数据库 |
| `put` | `(key, value, ttl) -> Result<()>` | 存储数据 |
| `get` | `(key) -> Result<T>` | 获取数据 |
| `delete` | `(key) -> Result<()>` | 删除数据 |
| `list` | `() -> Result<Vec<(String, T)>>` | 列出所有键值对 |
| `expire` | `(key, ttl) -> Result<()>` | 设置过期时间 |
| `contains_key` | `(key) -> bool` | 检查键是否存在 |
| `save` | `() -> Result<()>` | 持久化到磁盘 |
| `cleanup` | `() -> Result<usize>` | 清理过期数据 |
| `begin` | `() -> Transaction<'_>` | 开始事务 |
| `set_capacity` | `(Option<usize>) -> Result<usize>` | 设置 LRU 容量 |
| `get_capacity` | `() -> Result<Option<usize>>` | 获取当前容量 |
| `keys` | `() -> Result<Vec<String>>` | 获取所有键名 |
| `len` | `() -> Result<usize>` | 有效条目数 |
| `is_empty` | `() -> Result<bool>` | 是否为空 |
| `info` | `() -> Result<StoreInfo>` | 数据库统计信息 |

## 测试

```bash
# 运行全部测试
cargo test

# 运行特定模块测试
cargo test --lib          # 单元测试
cargo test --test integration  # 集成测试
```

## 代码质量

```bash
cargo fmt       # 格式化代码
cargo clippy    # 静态检查
```

## 技术栈

- **序列化**：serde + bincode
- **CLI**：clap (derive)
- **并发**：std::thread + Arc + RwLock + AtomicBool
- **网络**：std::net (TcpListener / TcpStream)
- **测试**：内置 test + tempfile

## 许可证

MIT
