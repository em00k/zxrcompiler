//! A map from text to values that remembers the order of insertion, as a
//! Python `dict` does. Output order follows dict order all over the Python
//! compiler, so nothing here is ever iterated in hash order.

use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct OrdMap<V> {
    index: HashMap<String, usize>,
    /// Removed entries leave a hole, so that positions stay valid.
    slots: Vec<Option<(String, V)>>,
    len: usize,
}

impl<V> Default for OrdMap<V> {
    fn default() -> Self {
        OrdMap { index: HashMap::new(), slots: Vec::new(), len: 0 }
    }
}

impl<V> OrdMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.index.get(key).map(|&i| &self.slots[i].as_ref().unwrap().1)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.index.get(key).map(|&i| &mut self.slots[i].as_mut().unwrap().1)
    }

    /// `d[key] = value`: a key already present keeps its place.
    pub fn insert(&mut self, key: &str, value: V) {
        match self.index.get(key) {
            Some(&i) => self.slots[i].as_mut().unwrap().1 = value,
            None => {
                self.index.insert(key.to_string(), self.slots.len());
                self.slots.push(Some((key.to_string(), value)));
                self.len += 1;
            }
        }
    }

    /// `d.pop(key, None)`.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        let i = self.index.remove(key)?;
        self.len -= 1;
        self.slots[i].take().map(|(_, v)| v)
    }

    pub fn clear(&mut self) {
        self.index.clear();
        self.slots.clear();
        self.len = 0;
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.slots.iter().flatten().map(|(k, v)| (k.as_str(), v))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }
}

#[cfg(test)]
mod tests {
    use super::OrdMap;

    #[test]
    fn keeps_insertion_order_like_a_dict() {
        let mut m = OrdMap::new();
        m.insert("b", 1);
        m.insert("a", 2);
        m.insert("c", 3);
        m.insert("b", 4);
        assert_eq!(m.iter().collect::<Vec<_>>(), [("b", &4), ("a", &2), ("c", &3)]);
        assert_eq!(m.remove("a"), Some(2));
        m.insert("a", 5);
        assert_eq!(m.keys().collect::<Vec<_>>(), ["b", "c", "a"]);
        assert_eq!(m.len(), 3);
    }
}
