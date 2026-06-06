//! LRU 淘汰策略 — 基于 arena 的双向链表实现

use std::collections::HashMap;

/// LRU 链表节点
#[derive(Debug, Clone)]
struct LruNode {
    key: String,
    prev: Option<usize>,
    next: Option<usize>,
}

/// LRU 访问追踪器
///
/// 使用 Vec 作为节点存储（arena），HashMap 做 key→index 映射。
/// 头节点为最近访问，尾节点为最久未访问。O(1) touch/evict/remove。
pub struct LruTracker {
    nodes: Vec<LruNode>,
    key_to_index: HashMap<String, usize>,
    head: Option<usize>,
    tail: Option<usize>,
}

impl Default for LruTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl LruTracker {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            key_to_index: HashMap::new(),
            head: None,
            tail: None,
        }
    }

    /// 访问一个键（新建或移到头部）
    pub fn touch(&mut self, key: &str) {
        if let Some(&index) = self.key_to_index.get(key) {
            self.detach(index);
            self.attach_to_head(index);
        } else {
            let index = self.nodes.len();
            self.nodes.push(LruNode {
                key: key.to_string(),
                prev: None,
                next: self.head,
            });
            self.key_to_index.insert(key.to_string(), index);
            self.attach_to_head(index);
        }
    }

    /// 移除一个键
    pub fn remove(&mut self, key: &str) {
        if let Some(&index) = self.key_to_index.get(key) {
            self.detach(index);
            self.key_to_index.remove(key);
        }
    }

    /// 淘汰最久未访问的键，返回被淘汰的键名
    pub fn evict_lru(&mut self) -> Option<String> {
        let tail_idx = self.tail?;
        let key = self.nodes[tail_idx].key.clone();
        self.key_to_index.remove(&key);
        self.detach(tail_idx);
        Some(key)
    }

    /// 当前追踪的键数量
    pub fn len(&self) -> usize {
        self.key_to_index.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.key_to_index.is_empty()
    }

    /// 是否包含某个键
    pub fn contains(&self, key: &str) -> bool {
        self.key_to_index.contains_key(key)
    }

    /// 从链表中摘下一个节点（不修改 key_to_index）
    fn detach(&mut self, index: usize) {
        let node = &self.nodes[index];
        let prev = node.prev;
        let next = node.next;

        if let Some(prev_idx) = prev {
            self.nodes[prev_idx].next = next;
        } else {
            self.head = next;
        }

        if let Some(next_idx) = next {
            self.nodes[next_idx].prev = prev;
        } else {
            self.tail = prev;
        }
    }

    /// 将节点附加到链表头部
    fn attach_to_head(&mut self, index: usize) {
        self.nodes[index].prev = None;
        self.nodes[index].next = self.head;

        if let Some(old_head) = self.head {
            self.nodes[old_head].prev = Some(index);
        } else {
            self.tail = Some(index);
        }

        self.head = Some(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_touch_order() {
        let mut lru = LruTracker::new();
        lru.touch("a");
        lru.touch("b");
        lru.touch("c");
        // 最近访问顺序: c(头) -> b -> a(尾)
        assert_eq!(lru.evict_lru(), Some("a".to_string()));
        assert_eq!(lru.evict_lru(), Some("b".to_string()));
        assert_eq!(lru.evict_lru(), Some("c".to_string()));
        assert_eq!(lru.evict_lru(), None);
    }

    #[test]
    fn test_touch_moves_to_head() {
        let mut lru = LruTracker::new();
        lru.touch("a");
        lru.touch("b");
        lru.touch("c");
        // 访问 a，a 移到头部: a(头) -> c -> b(尾)
        lru.touch("a");
        assert_eq!(lru.evict_lru(), Some("b".to_string()));
        assert_eq!(lru.evict_lru(), Some("c".to_string()));
        assert_eq!(lru.evict_lru(), Some("a".to_string()));
    }

    #[test]
    fn test_remove() {
        let mut lru = LruTracker::new();
        lru.touch("a");
        lru.touch("b");
        lru.touch("c");
        lru.remove("b");
        assert_eq!(lru.len(), 2);
        assert!(!lru.contains("b"));
        assert_eq!(lru.evict_lru(), Some("a".to_string()));
        assert_eq!(lru.evict_lru(), Some("c".to_string()));
    }

    #[test]
    fn test_remove_head() {
        let mut lru = LruTracker::new();
        lru.touch("a");
        lru.touch("b");
        lru.remove("b"); // b 是头
        assert_eq!(lru.len(), 1);
        assert_eq!(lru.evict_lru(), Some("a".to_string()));
    }

    #[test]
    fn test_remove_tail() {
        let mut lru = LruTracker::new();
        lru.touch("a");
        lru.touch("b");
        lru.remove("a"); // a 是尾
        assert_eq!(lru.len(), 1);
        assert_eq!(lru.evict_lru(), Some("b".to_string()));
    }

    #[test]
    fn test_single_element() {
        let mut lru = LruTracker::new();
        lru.touch("only");
        assert_eq!(lru.len(), 1);
        assert_eq!(lru.evict_lru(), Some("only".to_string()));
        assert_eq!(lru.len(), 0);
    }

    #[test]
    fn test_empty_evict() {
        let mut lru = LruTracker::new();
        assert_eq!(lru.evict_lru(), None);
    }

    #[test]
    fn test_touch_existing_then_remove() {
        let mut lru = LruTracker::new();
        lru.touch("a");
        lru.touch("b");
        // 反复 touch a
        lru.touch("a");
        lru.touch("a");
        lru.touch("a");
        assert_eq!(lru.len(), 2);
        // a 是头，b 是尾
        assert_eq!(lru.evict_lru(), Some("b".to_string()));
    }
}
