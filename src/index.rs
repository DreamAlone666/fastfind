use crate::ntfs::USNRecord;
use std::{collections::HashMap, path::MAIN_SEPARATOR_STR};

type Map = HashMap<u64, (u64, Box<str>)>;

pub struct Index {
    letter: String,
    map: Map,
}

impl Index {
    pub fn with_capacity(letter: String, capacity: usize) -> Self {
        Self {
            letter: letter,
            map: HashMap::with_capacity(capacity),
        }
    }

    pub fn insert(&mut self, record: USNRecord) {
        self.map
            .insert(record.frn, (record.parent_frn, record.filename.into()));
    }

    pub fn get_path(&self, mut frn: u64) -> Option<String> {
        let mut parts = Vec::new();
        while let Some((parent_frn, name)) = self.map.get(&frn) {
            parts.push(name.as_ref());
            frn = *parent_frn;
        }

        if parts.is_empty() {
            return None;
        }

        parts.push(&self.letter);
        parts.reverse();
        Some(parts.join(MAIN_SEPARATOR_STR))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn letter(&self) -> &str {
        &self.letter
    }
}

impl IntoIterator for Index {
    type Item = <Map as IntoIterator>::Item;
    type IntoIter = <Map as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.map.into_iter()
    }
}

impl<'a> IntoIterator for &'a Index {
    type Item = <&'a Map as IntoIterator>::Item;
    type IntoIter = <&'a Map as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}
