//! TCP 网络服务器 — 简单的文本协议

use crate::db::KvStore;
use crate::error::{Error, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// TCP 服务器
pub struct TcpServer {
    store: Arc<KvStore>,
    listener: TcpListener,
    running: Arc<AtomicBool>,
    addr: SocketAddr,
}

impl TcpServer {
    /// 创建服务器并绑定到地址
    pub fn bind(store: Arc<KvStore>, addr: &str) -> Result<Self> {
        let listener =
            TcpListener::bind(addr).map_err(|e| Error::Server(format!("bind failed: {}", e)))?;
        let addr = listener
            .local_addr()
            .map_err(|e| Error::Server(format!("local_addr failed: {}", e)))?;

        Ok(Self {
            store,
            listener,
            running: Arc::new(AtomicBool::new(true)),
            addr,
        })
    }

    /// 获取绑定的地址
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// 运行服务器（阻塞当前线程）
    pub fn run(&self) -> Result<()> {
        self.listener
            .set_nonblocking(false)
            .map_err(|e| Error::Server(format!("set_nonblocking failed: {}", e)))?;

        while self.running.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, _peer_addr)) => {
                    if !self.running.load(Ordering::SeqCst) {
                        break;
                    }
                    let store = Arc::clone(&self.store);
                    thread::spawn(move || {
                        handle_client(stream, store);
                    });
                }
                Err(e) => {
                    if self.running.load(Ordering::SeqCst) {
                        return Err(Error::Server(format!("accept error: {}", e)));
                    }
                    break;
                }
            }
        }
        Ok(())
    }

    /// 优雅关闭服务器
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
        // 自连接以唤醒 accept
        let _ = TcpStream::connect(self.addr);
    }
}

/// 处理单个客户端连接
fn handle_client(stream: TcpStream, store: Arc<KvStore>) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let reader = BufReader::new(reader_stream);
    let mut writer = &stream;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = process_command(trimmed, &store);
        let is_bye = response == "BYE";

        if writeln!(writer, "{}", response).is_err() {
            break;
        }
        if writer.flush().is_err() {
            break;
        }
        if is_bye {
            break;
        }
    }
}

/// 协议命令处理
fn process_command(input: &str, store: &Arc<KvStore>) -> String {
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts[0].to_uppercase();
    let args = if parts.len() > 1 { parts[1].trim() } else { "" };

    match cmd.as_str() {
        "SET" => cmd_set(args, store),
        "GET" => cmd_get(args, store),
        "DEL" => cmd_del(args, store),
        "LIST" => cmd_list(store),
        "EXPIRE" => cmd_expire(args, store),
        "KEYS" => cmd_keys(store),
        "INFO" => cmd_info(store),
        "CLEANUP" => cmd_cleanup(store),
        "QUIT" | "EXIT" => "BYE".to_string(),
        _ => format!("ERR unknown command: {}", cmd),
    }
}

fn cmd_set(args: &str, store: &Arc<KvStore>) -> String {
    // 格式: key value [ttl]
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return "ERR usage: SET key value [ttl]".to_string();
    }

    let key = parts[0].to_string();
    let rest = parts[1];

    // 检查最后一个 token 是否是 TTL（数字）
    let (value_str, ttl) = if let Some(last_space) = rest.rfind(' ') {
        let potential_ttl = &rest[last_space + 1..];
        if let Ok(t) = potential_ttl.parse::<u64>() {
            (&rest[..last_space], Some(t))
        } else {
            (rest, None)
        }
    } else if let Ok(t) = rest.parse::<u64>() {
        ("", Some(t))
    } else {
        (rest, None)
    };

    match store.put(&key, value_str.to_string(), ttl) {
        Ok(()) => "OK".to_string(),
        Err(e) => format!("ERR {}", e),
    }
}

fn cmd_get(args: &str, store: &Arc<KvStore>) -> String {
    let key = args.trim();
    if key.is_empty() {
        return "ERR usage: GET key".to_string();
    }

    match store.get::<String>(key) {
        Ok(value) => format!("\"{}\"", value),
        Err(_) => "(nil)".to_string(),
    }
}

fn cmd_del(args: &str, store: &Arc<KvStore>) -> String {
    let key = args.trim();
    if key.is_empty() {
        return "ERR usage: DEL key".to_string();
    }

    match store.delete(key) {
        Ok(()) => "OK".to_string(),
        Err(e) => format!("ERR {}", e),
    }
}

fn cmd_list(store: &Arc<KvStore>) -> String {
    match store.list::<String>() {
        Ok(items) => {
            if items.is_empty() {
                "(empty)".to_string()
            } else {
                let mut lines: Vec<String> = items
                    .iter()
                    .map(|(k, v)| format!("{}: \"{}\"", k, v))
                    .collect();
                lines.push("OK".to_string());
                lines.join("\n")
            }
        }
        Err(e) => format!("ERR {}", e),
    }
}

fn cmd_expire(args: &str, store: &Arc<KvStore>) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return "ERR usage: EXPIRE key ttl".to_string();
    }

    let key = parts[0];
    let ttl: u64 = match parts[1].parse() {
        Ok(t) => t,
        Err(_) => return "ERR ttl must be a number".to_string(),
    };

    match store.expire(key, ttl) {
        Ok(()) => "OK".to_string(),
        Err(e) => format!("ERR {}", e),
    }
}

fn cmd_keys(store: &Arc<KvStore>) -> String {
    match store.keys() {
        Ok(keys) => {
            if keys.is_empty() {
                "(empty)".to_string()
            } else {
                let mut lines = keys.clone();
                lines.push("OK".to_string());
                lines.join("\n")
            }
        }
        Err(e) => format!("ERR {}", e),
    }
}

fn cmd_info(store: &Arc<KvStore>) -> String {
    match store.info() {
        Ok(info) => {
            let cap_str = match info.capacity {
                Some(c) => c.to_string(),
                None => "unlimited".to_string(),
            };
            format!(
                "keys: {}\ntotal_entries: {}\ncapacity: {}\nfile: {}",
                info.key_count, info.total_entries, cap_str, info.file_path
            )
        }
        Err(e) => format!("ERR {}", e),
    }
}

fn cmd_cleanup(store: &Arc<KvStore>) -> String {
    match store.cleanup() {
        Ok(count) => format!("OK ({} entries removed)", count),
        Err(e) => format!("ERR {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    struct TestServer {
        running: Arc<AtomicBool>,
        addr: SocketAddr,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn shutdown(mut self) {
            self.running.store(false, Ordering::SeqCst);
            let _ = TcpStream::connect(self.addr);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.running.store(false, Ordering::SeqCst);
            let _ = TcpStream::connect(self.addr);
        }
    }

    fn setup_server() -> (Arc<KvStore>, TestServer) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("server_test.bin");
        std::mem::forget(temp_dir);

        let store = Arc::new(KvStore::new(&path).unwrap());
        let server = TcpServer::bind(Arc::clone(&store), "127.0.0.1:0").unwrap();
        let addr = server.local_addr();
        let running = Arc::clone(&server.running);

        let handle = thread::spawn(move || {
            let _ = server.run();
        });

        std::thread::sleep(Duration::from_millis(100));

        (
            store,
            TestServer {
                running,
                addr,
                handle: Some(handle),
            },
        )
    }

    fn send_cmd(addr: SocketAddr, cmd: &str) -> String {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        writeln!(stream, "{}", cmd).unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
        response.trim_end().to_string()
    }

    #[test]
    fn test_server_set_get() {
        let (_store, srv) = setup_server();
        let addr = srv.addr;

        let resp = send_cmd(addr, "SET hello world");
        assert_eq!(resp, "OK");

        let resp = send_cmd(addr, "GET hello");
        assert_eq!(resp, "\"world\"");

        srv.shutdown();
    }

    #[test]
    fn test_server_get_nonexistent() {
        let (_store, srv) = setup_server();
        let addr = srv.addr;

        let resp = send_cmd(addr, "GET nope");
        assert_eq!(resp, "(nil)");

        srv.shutdown();
    }

    #[test]
    fn test_server_delete() {
        let (_store, srv) = setup_server();
        let addr = srv.addr;

        send_cmd(addr, "SET tmp data");
        let resp = send_cmd(addr, "DEL tmp");
        assert_eq!(resp, "OK");

        let resp = send_cmd(addr, "GET tmp");
        assert_eq!(resp, "(nil)");

        srv.shutdown();
    }

    #[test]
    fn test_server_list() {
        let (_store, srv) = setup_server();
        let addr = srv.addr;

        send_cmd(addr, "SET k1 v1");
        send_cmd(addr, "SET k2 v2");
        let resp = send_cmd(addr, "LIST");
        assert!(resp.contains("k1: \"v1\""));
        assert!(resp.contains("k2: \"v2\""));

        srv.shutdown();
    }

    #[test]
    fn test_server_unknown_command() {
        let (_store, srv) = setup_server();
        let addr = srv.addr;

        let resp = send_cmd(addr, "BOGUS");
        assert!(resp.starts_with("ERR unknown command"));

        srv.shutdown();
    }

    #[test]
    fn test_server_quit() {
        let (_store, srv) = setup_server();
        let addr = srv.addr;

        let resp = send_cmd(addr, "QUIT");
        assert_eq!(resp, "BYE");

        srv.shutdown();
    }

    #[test]
    fn test_server_info() {
        let (_store, srv) = setup_server();
        let addr = srv.addr;

        send_cmd(addr, "SET info_key info_val");
        let resp = send_cmd(addr, "INFO");
        assert!(resp.contains("keys: 1"));
        assert!(resp.contains("capacity: unlimited"));

        srv.shutdown();
    }

    #[test]
    fn test_server_concurrent_clients() {
        let (_store, srv) = setup_server();
        let addr = srv.addr;

        let mut handles = vec![];
        for i in 0..5 {
            let a = addr;
            handles.push(thread::spawn(move || {
                let key = format!("concurrent_key{}", i);
                let cmd = format!("SET {} value{}", key, i);
                send_cmd(a, &cmd)
            }));
        }

        for h in handles {
            let resp = h.join().unwrap();
            assert_eq!(resp, "OK");
        }

        let resp = send_cmd(addr, "KEYS");
        assert!(resp.contains("concurrent_key0"));
        assert!(resp.contains("concurrent_key4"));

        srv.shutdown();
    }
}
