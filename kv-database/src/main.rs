//! KV Database — 命令行工具

use clap::{Parser, Subcommand};
use kv_database::{KvStore, Result, Transaction};
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "kv-database", version, about = "一个本地键值对数据库")]
struct Cli {
    /// 数据文件路径
    #[arg(short, long, default_value = "data.bin")]
    data: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 存储键值对 SET key value [ttl]
    Set {
        key: String,
        value: String,
        /// 过期时间（秒）
        #[arg(short, long)]
        ttl: Option<u64>,
    },
    /// 获取值 GET key
    Get { key: String },
    /// 删除键 DEL key
    Del { key: String },
    /// 列出所有键值对
    List,
    /// 设置过期时间 EXPIRE key ttl
    Expire { key: String, ttl: u64 },
    /// 清理过期数据
    Cleanup,
    /// 手动保存到文件
    Save,
    /// 显示数据库信息
    Info,
    /// 列出所有键名
    Keys,
    /// 启动 TCP 服务器
    Serve {
        /// 监听端口
        #[arg(short, long, default_value = "6379")]
        port: u16,
        /// 绑定地址
        #[arg(short, long, default_value = "127.0.0.1")]
        bind: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let store = match KvStore::open(&cli.data) {
        Ok(s) => s,
        Err(_) => {
            // 文件不存在则创建新实例
            match KvStore::new(&cli.data) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to open database: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    match cli.command {
        Some(cmd) => run_command(cmd, &store),
        None => run_repl(&cli.data),
    }
}

fn run_command(cmd: Commands, store: &KvStore) {
    match cmd {
        Commands::Set { key, value, ttl } => match store.put(&key, &value, ttl) {
            Ok(()) => {
                println!("OK");
                let _ = store.save();
            }
            Err(e) => eprintln!("ERR {}", e),
        },
        Commands::Get { key } => match store.get::<String>(&key) {
            Ok(value) => println!("\"{}\"", value),
            Err(_) => println!("(nil)"),
        },
        Commands::Del { key } => match store.delete(&key) {
            Ok(()) => {
                println!("OK");
                let _ = store.save();
            }
            Err(e) => eprintln!("ERR {}", e),
        },
        Commands::List => match store.list::<String>() {
            Ok(items) => {
                if items.is_empty() {
                    println!("(empty)");
                } else {
                    for (key, value) in &items {
                        println!("  {}: \"{}\"", key, value);
                    }
                }
            }
            Err(e) => eprintln!("ERR {}", e),
        },
        Commands::Expire { key, ttl } => match store.expire(&key, ttl) {
            Ok(()) => {
                println!("OK");
                let _ = store.save();
            }
            Err(e) => eprintln!("ERR {}", e),
        },
        Commands::Cleanup => match store.cleanup() {
            Ok(count) => {
                println!("Cleaned {} expired entries", count);
                let _ = store.save();
            }
            Err(e) => eprintln!("ERR {}", e),
        },
        Commands::Save => match store.save() {
            Ok(()) => println!("Saved successfully"),
            Err(e) => eprintln!("ERR {}", e),
        },
        Commands::Info => match store.info() {
            Ok(info) => {
                let cap_str = match info.capacity {
                    Some(c) => c.to_string(),
                    None => "unlimited".to_string(),
                };
                println!("File:       {}", info.file_path);
                println!("Keys:       {}", info.key_count);
                println!("Entries:    {}", info.total_entries);
                println!("Capacity:   {}", cap_str);
            }
            Err(e) => eprintln!("ERR {}", e),
        },
        Commands::Keys => match store.keys() {
            Ok(keys) => {
                if keys.is_empty() {
                    println!("(empty)");
                } else {
                    for key in &keys {
                        println!("  {}", key);
                    }
                }
            }
            Err(e) => eprintln!("ERR {}", e),
        },
        Commands::Serve { port, bind } => {
            let addr = format!("{}:{}", bind, port);
            let store = std::sync::Arc::new(
                KvStore::open(&store.info().unwrap().file_path)
                    .unwrap_or_else(|_| KvStore::new("data.bin").unwrap()),
            );
            // 使用已有的 store 包装为 Arc
            let store = {
                // 获取路径并重新打开
                let path = match store.info() {
                    Ok(info) => info.file_path,
                    Err(_) => "data.bin".to_string(),
                };
                std::sync::Arc::new(
                    KvStore::open(&path).unwrap_or_else(|_| KvStore::new(&path).unwrap()),
                )
            };

            let server = match kv_database::TcpServer::bind(store, &addr) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to start server: {}", e);
                    std::process::exit(1);
                }
            };

            println!("KV Database server listening on {}", server.local_addr());
            println!("Press Ctrl-C to stop");

            if let Err(e) = server.run() {
                eprintln!("Server error: {}", e);
            }
        }
    }
}

fn run_repl(data_path: &str) {
    println!("KV Database REPL");
    println!("Type QUIT to exit, HELP for commands");
    println!("Data file: {}", data_path);

    let store = KvStore::open(data_path).unwrap_or_else(|_| KvStore::new(data_path).unwrap());
    let mut txn: Option<Transaction<'_>> = None;
    let stdin = io::stdin();
    let mut line = String::new();

    loop {
        // 显示提示符
        let prompt = if txn.is_some() { "txn> " } else { "> " };
        print!("{}", prompt);
        io::stdout().flush().unwrap();

        line.clear();
        if stdin.read_line(&mut line).is_err() {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        let cmd = parts[0].to_uppercase();
        let args = if parts.len() > 1 { parts[1].trim() } else { "" };

        match cmd.as_str() {
            "QUIT" | "EXIT" => {
                if txn.is_some() {
                    println!("Warning: active transaction will be rolled back");
                }
                if let Err(e) = store.save() {
                    println!("Warning: failed to save: {}", e);
                }
                println!("Goodbye!");
                break;
            }
            "HELP" => repl_help(),
            "BEGIN" => {
                if txn.is_some() {
                    println!("ERR: transaction already active");
                } else {
                    txn = Some(store.begin());
                    println!("OK (transaction started)");
                }
            }
            "COMMIT" => {
                if let Some(t) = txn.take() {
                    match t.commit() {
                        Ok(()) => println!("OK (transaction committed)"),
                        Err(e) => println!("ERR {}", e),
                    }
                } else {
                    println!("ERR: no active transaction");
                }
            }
            "ROLLBACK" => {
                if let Some(t) = txn.take() {
                    t.rollback();
                    println!("OK (transaction rolled back)");
                } else {
                    println!("ERR: no active transaction");
                }
            }
            "SET" => repl_set(&store, &mut txn, args),
            "GET" => repl_get(&store, &mut txn, args),
            "DEL" => repl_del(&store, &mut txn, args),
            "LIST" => repl_list(&store, &mut txn),
            "EXPIRE" => repl_expire(&store, &mut txn, args),
            "KEYS" => repl_keys(&store, &mut txn),
            "INFO" => repl_info(&store, &mut txn),
            "CLEANUP" => repl_cleanup(&store, &mut txn, args),
            "SAVE" => repl_save(&store, &mut txn, args),
            "CAPACITY" => repl_capacity(&store, &mut txn, args),
            _ => println!("ERR: unknown command '{}'. Type HELP for commands", cmd),
        }
    }
}

fn repl_help() {
    println!("Commands:");
    println!("  SET key value [ttl]  — store a key-value pair");
    println!("  GET key              — retrieve a value");
    println!("  DEL key              — delete a key");
    println!("  LIST                 — list all key-value pairs");
    println!("  KEYS                 — list all keys");
    println!("  EXPIRE key ttl       — set expiration time");
    println!("  CLEANUP              — remove expired entries");
    println!("  SAVE                 — persist to file");
    println!("  INFO                 — database statistics");
    println!("  CAPACITY [n]         — get/set max capacity (LRU)");
    println!("Transaction commands:");
    println!("  BEGIN                — start a transaction");
    println!("  COMMIT               — commit transaction");
    println!("  ROLLBACK             — rollback transaction");
    println!("  QUIT                 — exit REPL");
}

fn parse_set_args(args: &str) -> (&str, &str, Option<u64>) {
    let args = args.trim();
    // 找到第一个空格分隔 key 和 value+ttl
    let (key, rest) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => return (args, "", None),
    };

    if rest.is_empty() {
        return (key, rest, None);
    }

    // 处理 --ttl / -t 标志语法: SET key value --ttl 3600
    for flag in &[" --ttl ", " -t "] {
        if let Some(flag_pos) = rest.find(flag) {
            let value_part = &rest[..flag_pos];
            let ttl_part = &rest[flag_pos + flag.len()..];
            if let Ok(t) = ttl_part.parse::<u64>() {
                return (key, value_part, Some(t));
            }
        }
    }

    // 末尾纯数字 TTL: SET key value 3600
    if let Some(last_space) = rest.rfind(' ') {
        let potential_ttl = &rest[last_space + 1..];
        if let Ok(t) = potential_ttl.parse::<u64>() {
            return (key, &rest[..last_space], Some(t));
        }
    }

    (key, rest, None)
}

fn repl_set(store: &KvStore, txn: &mut Option<Transaction<'_>>, args: &str) {
    let (key, value, ttl) = parse_set_args(args);
    if key.is_empty() {
        println!("ERR usage: SET key value [ttl]");
        return;
    }

    let result = if let Some(ref mut t) = txn {
        t.put(key, value.to_string(), ttl)
    } else {
        store.put(key, value.to_string(), ttl)
    };

    match result {
        Ok(()) => println!("OK"),
        Err(e) => println!("ERR {}", e),
    }
}

fn repl_get(store: &KvStore, txn: &mut Option<Transaction<'_>>, args: &str) {
    let key = args.trim();
    if key.is_empty() {
        println!("ERR usage: GET key");
        return;
    }

    let result: Result<String> = if let Some(ref t) = txn {
        t.get(key)
    } else {
        store.get(key)
    };

    match result {
        Ok(value) => println!("\"{}\"", value),
        Err(_) => println!("(nil)"),
    }
}

fn repl_del(store: &KvStore, txn: &mut Option<Transaction<'_>>, args: &str) {
    let key = args.trim();
    if key.is_empty() {
        println!("ERR usage: DEL key");
        return;
    }

    let result = if let Some(ref mut t) = txn {
        t.delete(key)
    } else {
        store.delete(key)
    };

    match result {
        Ok(()) => println!("OK"),
        Err(e) => println!("ERR {}", e),
    }
}

fn repl_list(store: &KvStore, txn: &mut Option<Transaction<'_>>) {
    let result: Result<Vec<(String, String)>> = if let Some(ref t) = txn {
        t.list()
    } else {
        store.list()
    };

    match result {
        Ok(items) => {
            if items.is_empty() {
                println!("(empty)");
            } else {
                for (key, value) in &items {
                    println!("  {}: \"{}\"", key, value);
                }
            }
        }
        Err(e) => println!("ERR {}", e),
    }
}

fn repl_expire(store: &KvStore, txn: &mut Option<Transaction<'_>>, args: &str) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        println!("ERR usage: EXPIRE key ttl");
        return;
    }
    let key = parts[0];
    let ttl: u64 = match parts[1].parse() {
        Ok(t) => t,
        Err(_) => {
            println!("ERR ttl must be a number");
            return;
        }
    };

    let result = if let Some(ref mut t) = txn {
        t.expire(key, ttl)
    } else {
        store.expire(key, ttl)
    };

    match result {
        Ok(()) => println!("OK"),
        Err(e) => println!("ERR {}", e),
    }
}

fn repl_keys(store: &KvStore, txn: &mut Option<Transaction<'_>>) {
    let result = if let Some(ref t) = txn {
        Ok(t.keys())
    } else {
        store.keys()
    };

    match result {
        Ok(keys) => {
            if keys.is_empty() {
                println!("(empty)");
            } else {
                for key in &keys {
                    println!("  {}", key);
                }
            }
        }
        Err(e) => println!("ERR {}", e),
    }
}

fn repl_info(store: &KvStore, txn: &mut Option<Transaction<'_>>) {
    // 事务中 info 仍然从 store 获取（事务不影响存储元数据）
    if txn.is_some() {
        println!("(transaction active — showing database state)");
    }
    match store.info() {
        Ok(info) => {
            let cap_str = match info.capacity {
                Some(c) => c.to_string(),
                None => "unlimited".to_string(),
            };
            println!("File:       {}", info.file_path);
            println!("Keys:       {}", info.key_count);
            println!("Entries:    {}", info.total_entries);
            println!("Capacity:   {}", cap_str);
        }
        Err(e) => println!("ERR {}", e),
    }
}

fn repl_cleanup(store: &KvStore, txn: &mut Option<Transaction<'_>>, args: &str) {
    if txn.is_some() {
        println!("ERR: cannot cleanup while transaction is active");
        return;
    }
    match store.cleanup() {
        Ok(count) => println!("Cleaned {} expired entries", count),
        Err(e) => println!("ERR {}", e),
    }
    let _ = args;
}

fn repl_save(store: &KvStore, txn: &mut Option<Transaction<'_>>, args: &str) {
    if txn.is_some() {
        println!("ERR: cannot save while transaction is active");
        return;
    }
    match store.save() {
        Ok(()) => println!("Saved successfully"),
        Err(e) => println!("ERR {}", e),
    }
    let _ = args;
}

fn repl_capacity(store: &KvStore, txn: &mut Option<Transaction<'_>>, args: &str) {
    let args = args.trim();
    if args.is_empty() {
        match store.get_capacity() {
            Ok(cap) => match cap {
                Some(c) => println!("Capacity: {}", c),
                None => println!("Capacity: unlimited"),
            },
            Err(e) => println!("ERR {}", e),
        }
    } else {
        if txn.is_some() {
            println!("ERR: cannot change capacity while transaction is active");
            return;
        }
        let new_cap = if args.eq_ignore_ascii_case("none") || args == "0" {
            None
        } else {
            match args.parse::<usize>() {
                Ok(n) => Some(n),
                Err(_) => {
                    println!("ERR: capacity must be a number, or 'none'");
                    return;
                }
            }
        };

        match store.set_capacity(new_cap) {
            Ok(evicted) => match new_cap {
                Some(c) => println!("Capacity set to {} ({} entries evicted)", c, evicted),
                None => println!("Capacity removed (unlimited)"),
            },
            Err(e) => println!("ERR {}", e),
        }
    }
}
