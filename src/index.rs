mod find;

use anyhow::Result;
use log::debug;
use std::collections::HashMap;

use crate::ntfs::{UsnRecord, Volume};
use find::FindIter;

pub struct Index {
    driver: String,
    map: HashMap<u64, (u64, Box<str>)>,
}

impl Index {
    pub fn with_capacity(driver: String, capacity: usize) -> Self {
        Self {
            driver,
            map: HashMap::with_capacity(capacity),
        }
    }

    pub fn try_from_volume(vol: &Volume) -> Result<Self> {
        let mut index = Self::with_capacity(vol.driver().to_string(), 10_0000);
        let mut count: u64 = 0;
        for record in vol.file_records::<4096>() {
            index.insert(record?);
            count += 1;
        }
        debug!("{} 盘文件记录 {count} 条", vol.driver());
        Ok(index)
    }

    pub fn insert(&mut self, record: UsnRecord) {
        self.map
            .insert(record.frn, (record.parent_frn, record.filename.into()));
    }

    pub fn driver(&self) -> &str {
        &self.driver
    }

    pub fn find_iter<'a>(&'a self, sub: &'a str) -> FindIter<'a> {
        FindIter::new(self, sub)
    }
}
