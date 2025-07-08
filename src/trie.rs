use std::sync::Arc;
use tokio::sync::RwLock;

type TrieValue = Arc<RwLock<Vec<u8>>>;

#[derive(Debug, Default)]
struct TrieNode {
    key: u8,
    next_idx: usize,
    child_first_idx: usize,
    child_last_idx: usize,
    data_idx: Option<usize>,
}

pub struct Trie {
    values: Vec<TrieValue>,
    values_free_list: Vec<usize>,
    nodes: Vec<TrieNode>,
    nodes_free_list: Vec<usize>,
}

impl Trie {
    pub fn new() -> Self {
        let mut trie = Self {
            values: Vec::new(),
            values_free_list: Vec::new(),
            nodes: Vec::new(),
            nodes_free_list: Vec::new(),
        };
        trie.nodes.push(TrieNode::default());
        return trie;
    }

    fn _insert(
        self: &mut Self,
        key: &[u8],
        value: TrieValue,
        depth: usize,
        parent_idx: usize,
    ) -> Option<TrieValue> {
        let mut result = None;

        // Somewhat inefficient for characters in the 128+ range, since a single
        // code-point would result in multiple nested nodes, but the overhead is
        // not too bad, we don't pay the price for utf-8 decoding, and there are
        // other, more impactful optimisations that would be worth exploring now
        // - e.g. collapsing chains of data-less nodes.
        let ch = key[depth];
        let first_sibling_idx = self.nodes[parent_idx].child_first_idx;
        let mut node_idx = first_sibling_idx;

        while node_idx > 0 && self.nodes[node_idx].key != ch {
            node_idx = self.nodes[node_idx].next_idx;
        }

        if node_idx == 0 {
            node_idx = self.nodes.len();
            if let Some(free_idx) = self.nodes_free_list.pop() {
                node_idx = free_idx;
            }
            let node = TrieNode {
                key: ch,
                child_last_idx: 0,
                next_idx: 0,
                child_first_idx: 0,
                data_idx: None,
            };
            if node_idx < self.nodes.len() {
                self.nodes[node_idx] = node;
            } else {
                self.nodes.push(node);
            }
            if first_sibling_idx > 0 {
                let last_sibling_idx = self.nodes[parent_idx].child_last_idx;
                self.nodes[last_sibling_idx].next_idx = node_idx;
            } else {
                self.nodes[parent_idx].child_first_idx = node_idx;
            }
            self.nodes[parent_idx].child_last_idx = node_idx;
        }

        if depth < key.len() - 1 {
            result = self._insert(key, value, depth + 1, node_idx);
        } else {
            let node = &mut self.nodes[node_idx];
            if let Some(data_idx) = node.data_idx {
                result = Some(self.values[data_idx].clone());
                self.values[data_idx] = value;
            } else {
                let mut data_idx = self.values.len();
                if let Some(free_idx) = self.values_free_list.pop() {
                    data_idx = free_idx;
                }
                if data_idx < self.values.len() {
                    self.values[data_idx] = value;
                } else {
                    self.values.push(value);
                }
                node.data_idx = Some(data_idx);
            };
        }

        return result;
    }

    pub fn insert(self: &mut Self, key: &String, value: TrieValue) -> Option<TrieValue> {
        return self._insert(key.as_bytes(), value, 0, 0);
    }

    fn _get_idx(self: &Self, key: &[u8], depth: usize, parent_idx: usize) -> (usize, usize) {
        let mut result = (0, 0);

        let ch = key[depth];
        let mut node_idx = self.nodes[parent_idx].child_first_idx;

        while node_idx > 0 && self.nodes[node_idx].key != ch {
            node_idx = self.nodes[node_idx].next_idx;
        }

        if node_idx > 0 {
            if depth < key.len() - 1 {
                result = self._get_idx(key, depth + 1, node_idx);
            } else {
                return (node_idx, depth);
            }
        }

        return result;
    }

    pub fn get(self: &Self, key: &String) -> Option<TrieValue> {
        let (idx, _) = self._get_idx(key.as_bytes(), 0, 0);
        let mut result = None;
        if idx > 0 {
            if let Some(data_idx) = self.nodes[idx].data_idx {
                result = Some(self.values[data_idx].clone());
            }
        }
        return result;
    }

    fn _remove(self: &mut Self, key: &[u8], depth: usize, parent_idx: usize) -> Option<TrieValue> {
        let mut result = None;

        let ch = key[depth];
        let mut node_idx = self.nodes[parent_idx].child_first_idx;
        let mut prev_sibling_idx = 0;

        while node_idx > 0 && self.nodes[node_idx].key != ch {
            prev_sibling_idx = node_idx;
            node_idx = self.nodes[node_idx].next_idx;
        }

        if node_idx > 0 {
            if depth < key.len() - 1 {
                result = self._remove(key, depth + 1, node_idx);
            } else {
                // Mark value slot as free and remove its index
                if let Some(data_idx) = self.nodes[node_idx].data_idx {
                    self.values_free_list.push(data_idx);
                }
                self.nodes[node_idx].data_idx = None;
            }

            // Cleanup on the way up
            if self.nodes[node_idx].data_idx.is_some() {
                // Never remove
            } else if self.nodes[node_idx].child_first_idx > 0 {
                // Never remove
            } else {
                if node_idx == self.nodes[parent_idx].child_first_idx {
                    let next_sibling_idx = self.nodes[node_idx].next_idx;
                    if next_sibling_idx > 0 {
                        // Replace parent's first child with next sibling if available
                        self.nodes[parent_idx].child_first_idx = next_sibling_idx;
                    } else {
                        // Clear parent's children indeces if no siblings
                        self.nodes[parent_idx].child_first_idx = 0;
                        self.nodes[parent_idx].child_last_idx = 0;
                    }
                } else if node_idx == self.nodes[parent_idx].child_last_idx {
                    // Clear prev sibling's next idx and replace parent's last child idx
                    self.nodes[prev_sibling_idx].next_idx = 0;
                    self.nodes[parent_idx].child_last_idx = prev_sibling_idx;
                } else {
                    // Neither last nor first child
                    self.nodes[prev_sibling_idx].next_idx = self.nodes[node_idx].next_idx;
                }
            }
        }

        return result;
    }

    pub fn remove(self: &mut Self, key: &String) -> Option<TrieValue> {
        return self._remove(key.as_bytes(), 0, 0);
    }

    fn _keys(
        self: &Self,
        depth: usize,
        parent_idx: usize,
        prefix_buffer: &mut Vec<u8>,
        result_buffer: &mut Vec<String>,
    ) {
        let mut node_idx = self.nodes[parent_idx].child_first_idx;

        while node_idx > 0 {
            let key = self.nodes[node_idx].key;
            if depth >= prefix_buffer.len() {
                prefix_buffer.push(key);
            } else {
                prefix_buffer[depth] = key;
            }
            // Descend to leaf nodes
            if self.nodes[node_idx].child_first_idx > 0 {
                self._keys(depth + 1, node_idx, prefix_buffer, result_buffer);
            } else {
                let key =
                    unsafe { String::from_utf8_unchecked(prefix_buffer[0..depth + 1].to_vec()) };
                result_buffer.push(key);
            }
            node_idx = self.nodes[node_idx].next_idx;
        }
    }

    pub fn keys(self: &Self) -> Vec<String> {
        // TODO(Jakub): consider turning into an iterator
        let mut result = Vec::new();
        self._keys(0, 0, &mut Vec::new(), &mut result);
        return result;
    }

    pub fn keys_by_prefix(self: &Self, prefix: &String) -> Vec<String> {
        let mut result = Vec::new();
        let mut prefix_buffer = Vec::new();
        let (prefix_root_idx, depth) = self._get_idx(prefix.as_bytes(), 0, 0);
        if prefix_root_idx > 0 {
            if self.nodes[prefix_root_idx].data_idx.is_some() {
                result.push(prefix.clone());
            }
            prefix_buffer.extend(prefix.as_bytes());
            self._keys(depth + 1, prefix_root_idx, &mut prefix_buffer, &mut result);
        }
        return result;
    }
}
