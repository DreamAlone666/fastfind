mod iter_find;

use log::debug;
use std::{collections::HashMap, path::MAIN_SEPARATOR_STR};

use crate::ntfs::{UsnRecord, Volume};
use iter_find::IterFind;

type Map = HashMap<u64, (u64, Box<str>)>;

pub struct Index {
    driver: String,
    map: Map,
}

impl Index {
    pub fn with_capacity(driver: String, capacity: usize) -> Self {
        Self {
            driver,
            map: HashMap::with_capacity(capacity),
        }
    }

    pub fn insert(&mut self, record: UsnRecord) {
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

        parts.push(&self.driver);
        parts.reverse();
        Some(parts.join(MAIN_SEPARATOR_STR))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn driver(&self) -> &str {
        &self.driver
    }

    pub fn iter_find<'a>(&'a self, sub: &'a str) -> IterFind<'a> {
        IterFind::new(self, sub)
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

impl TryFrom<&Volume> for Index {
    type Error = anyhow::Error;

    fn try_from(vol: &Volume) -> Result<Self, Self::Error> {
        let mut index = Self::with_capacity(vol.driver().to_string(), 10_0000);
        let mut count: u64 = 0;
        for record in vol.iter_usn_record::<4096>() {
            index.insert(record?);
            count += 1;
        }
        debug!("IterUsnRecord({:?}) {count} 条", vol.driver());
        Ok(index)
    }
}
