//! String-key trie (prefix tree).

use rayon::prelude::*;
use std::collections::BTreeMap;

struct Node<V> {
  value: Option<V>,
  children: BTreeMap<char, Node<V>>,
}

impl<V> Default for Node<V> {
  fn default() -> Self {
    Self {
      value: None,
      children: BTreeMap::new(),
    }
  }
}

/// Prefix map keyed by `str` slices.
pub struct Trie<V> {
  root: Node<V>,
}

impl<V> Default for Trie<V> {
  fn default() -> Self {
    Self::empty()
  }
}

impl<V> Trie<V> {
  /// Empty trie (no keys).
  #[inline]
  pub fn empty() -> Self {
    Self {
      root: Node::default(),
    }
  }

  fn navigate_mut<'a>(node: &'a mut Node<V>, key: &str) -> &'a mut Node<V> {
    let mut cur = node;
    for ch in key.chars() {
      cur = cur.children.entry(ch).or_default();
    }
    cur
  }

  fn navigate_ref<'a>(node: &'a Node<V>, key: &str) -> Option<&'a Node<V>> {
    let mut cur = node;
    for ch in key.chars() {
      cur = cur.children.get(&ch)?;
    }
    Some(cur)
  }

  /// Sets `key` → `value`; returns the previous value at `key`, if any.
  pub fn insert(&mut self, key: &str, value: V) -> Option<V> {
    let n = Self::navigate_mut(&mut self.root, key);
    n.value.replace(value)
  }

  /// Removes `key` and returns its value, if it existed.
  pub fn remove(&mut self, key: &str) -> Option<V> {
    Self::navigate_mut_optional(&mut self.root, key).and_then(|n| n.value.take())
  }

  fn navigate_mut_optional<'a>(node: &'a mut Node<V>, key: &str) -> Option<&'a mut Node<V>> {
    let mut cur = node;
    for ch in key.chars() {
      cur = cur.children.get_mut(&ch)?;
    }
    Some(cur)
  }

  /// Borrows the value at exact `key`, if present.
  pub fn get(&self, key: &str) -> Option<&V> {
    Self::navigate_ref(&self.root, key).and_then(|n| n.value.as_ref())
  }

  /// Whether an exact `key` is stored.
  pub fn has(&self, key: &str) -> bool {
    self.get(key).is_some()
  }

  /// Longest stored key that is a prefix of `key` (may be empty string if root holds a value).
  pub fn longest_prefix_of<'a>(&self, key: &'a str) -> Option<&'a str> {
    let mut cur = &self.root;
    let mut end: Option<usize> = None;
    if cur.value.is_some() {
      end = Some(0);
    }
    for (byte_idx, c) in key.char_indices() {
      let Some(next) = cur.children.get(&c) else {
        break;
      };
      cur = next;
      let after = byte_idx + c.len_utf8();
      if cur.value.is_some() {
        end = Some(after);
      }
    }
    end.map(|e| &key[..e])
  }

  /// Count of stored keys (nodes with a value).
  pub fn size(&self) -> usize {
    Self::count_nodes(&self.root)
  }

  /// Like [`Self::size`], but sums key counts in parallel per subtree. Requires `V: Sync` so child
  /// nodes can be read from multiple Rayon workers.
  pub fn size_par(&self) -> usize
  where
    V: Sync,
  {
    Self::count_nodes_par(&self.root)
  }

  fn count_nodes(node: &Node<V>) -> usize {
    let here = if node.value.is_some() { 1 } else { 0 };
    here + node.children.values().map(Self::count_nodes).sum::<usize>()
  }

  fn count_nodes_par(node: &Node<V>) -> usize
  where
    V: Sync,
  {
    let here = if node.value.is_some() { 1 } else { 0 };
    let rest: usize = node
      .children
      .values()
      .par_bridge()
      .map(Self::count_nodes_par)
      .sum();
    here + rest
  }

  fn collect_keys(node: &Node<V>, prefix: &str, out: &mut Vec<String>) {
    if node.value.is_some() {
      out.push(prefix.to_string());
    }
    for (ch, child) in &node.children {
      let mut p = prefix.to_string();
      p.push(*ch);
      Self::collect_keys(child, &p, out);
    }
  }

  /// All full keys under the subtrie rooted at `prefix`.
  pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(start) = Self::navigate_ref(&self.root, prefix) else {
      return out;
    };
    Self::collect_keys(start, prefix, &mut out);
    out
  }

  fn collect_entries<'a>(node: &'a Node<V>, prefix: &str, out: &mut Vec<(String, &'a V)>) {
    if let Some(ref v) = node.value {
      out.push((prefix.to_string(), v));
    }
    for (ch, child) in &node.children {
      let mut p = prefix.to_string();
      p.push(*ch);
      Self::collect_entries(child, &p, out);
    }
  }

  /// `(key, value)` pairs for every key under `prefix`.
  pub fn entries_with_prefix(&self, prefix: &str) -> Vec<(String, &V)> {
    let mut out = Vec::new();
    let Some(start) = Self::navigate_ref(&self.root, prefix) else {
      return out;
    };
    Self::collect_entries(start, prefix, &mut out);
    out
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn trie_longest_prefix_of_empty_returns_none_without_root_value() {
    let trie = Trie::<i32>::empty();
    assert_eq!(trie.longest_prefix_of("abc"), None);
  }

  #[test]
  fn trie_keys_with_prefix_finds_all_matches() {
    let mut trie = Trie::empty();
    trie.insert("foo", 1);
    trie.insert("food", 2);
    trie.insert("bar", 3);
    let mut ks = trie.keys_with_prefix("fo");
    ks.sort();
    assert_eq!(ks, vec!["foo".to_string(), "food".to_string()]);
    assert_eq!(trie.size(), 3);
    assert_eq!(trie.size(), trie.size_par());
  }

  #[test]
  fn trie_default_creates_empty_trie() {
    let t = Trie::<i32>::default();
    assert_eq!(t.size(), 0);
    assert!(!t.has("anything"));
  }

  #[test]
  fn trie_insert_returns_previous_value() {
    let mut t = Trie::empty();
    assert_eq!(t.insert("foo", 1i32), None);
    assert_eq!(t.insert("foo", 2), Some(1));
    assert_eq!(t.get("foo"), Some(&2));
  }

  #[test]
  fn trie_remove_existing_key_returns_value() {
    let mut t = Trie::empty();
    t.insert("hello", 42i32);
    assert_eq!(t.remove("hello"), Some(42));
    assert!(!t.has("hello"));
    assert_eq!(t.size(), 0);
  }

  #[test]
  fn trie_remove_missing_key_returns_none() {
    let mut t = Trie::<i32>::empty();
    assert_eq!(t.remove("xyz"), None);
  }

  #[test]
  fn trie_has_matches_exact_keys_only() {
    let mut t = Trie::empty();
    t.insert("hello", 1i32);
    assert!(t.has("hello"));
    assert!(!t.has("hell"));
    assert!(!t.has("helloo"));
  }

  #[test]
  fn trie_longest_prefix_of_returns_longest_match() {
    let mut t = Trie::empty();
    t.insert("he", 1i32);
    t.insert("hello", 2);
    assert_eq!(t.longest_prefix_of("hello world"), Some("hello"));
    assert_eq!(t.longest_prefix_of("hel"), Some("he"));
    assert_eq!(t.longest_prefix_of("xyz"), None);
  }

  #[test]
  fn trie_longest_prefix_of_matches_root_value() {
    let mut t = Trie::empty();
    t.insert("", 0i32);
    assert_eq!(t.longest_prefix_of("anything"), Some(""));
  }

  #[test]
  fn trie_entries_with_prefix_returns_matching_pairs() {
    let mut t = Trie::empty();
    t.insert("foo", 1i32);
    t.insert("food", 2);
    t.insert("bar", 3);
    let mut entries = t.entries_with_prefix("foo");
    entries.sort_by_key(|(k, _)| k.clone());
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0], ("foo".to_string(), &1));
    assert_eq!(entries[1], ("food".to_string(), &2));
  }

  #[test]
  fn trie_entries_with_prefix_no_match_returns_empty() {
    let t = Trie::<i32>::empty();
    assert!(t.entries_with_prefix("xyz").is_empty());
  }

  #[test]
  fn trie_keys_with_prefix_no_match_returns_empty() {
    let t = Trie::<i32>::empty();
    assert!(t.keys_with_prefix("xyz").is_empty());
  }
}
