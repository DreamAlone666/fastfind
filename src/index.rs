mod find;

use anyhow::Result;
use log::debug;
use std::collections::HashMap;
use windows::Win32::System::Ioctl::{
    USN_REASON_CLOSE, USN_REASON_FILE_CREATE, USN_REASON_FILE_DELETE, USN_REASON_RENAME_NEW_NAME,
};

use crate::ntfs::{UsnRecord, Volume};
use find::FindIter;

type V = (u64, Box<str>);

pub struct Index {
    driver: String,
    map: HashMap<u64, V>,
    usn: i64,
}

impl Index {
    pub fn with_capacity(driver: String, usn: i64, capacity: usize) -> Self {
        Self {
            driver,
            map: HashMap::with_capacity(capacity),
            usn,
        }
    }

    pub fn try_from_volume(vol: &Volume) -> Result<Self> {
        let usn = vol.usn_journal_data()?.next_usn;
        let mut index = Self::with_capacity(vol.driver().to_string(), usn, 10_0000);
        let mut count: u64 = 0;
        for record in vol.file_records::<4096>() {
            index.insert(record?);
            count += 1;
        }
        debug!("{} 盘文件记录 {count} 条", vol.driver());
        Ok(index)
    }

    pub fn insert(&mut self, record: UsnRecord) -> Option<V> {
        self.map
            .insert(record.frn, (record.parent_frn, record.filename.into()))
    }

    pub fn driver(&self) -> &str {
        &self.driver
    }

    pub fn find_iter<'a>(&'a self, sub: &'a str) -> FindIter<'a> {
        FindIter::new(self, sub)
    }

    pub fn sync(&mut self, vol: &Volume) -> Result<()> {
        let id = vol.usn_journal_data()?.id;
        let mut usn_records = vol.usn_records_from::<4096>(id, self.usn);
        while let Some(res) = usn_records.next() {
            let record = res?;
            // 只匹配文件关闭时的事件
            match record.reason ^ USN_REASON_CLOSE {
                USN_REASON_FILE_CREATE => {
                    debug!("Index({:?})：创建 {:?}", self.driver(), record.filename);
                    self.insert(record);
                }
                USN_REASON_FILE_DELETE => {
                    self.map.remove(&record.frn);
                    debug!("Index({:?})：删除 {:?}", self.driver(), record.filename);
                }
                USN_REASON_RENAME_NEW_NAME => {
                    let frn = record.frn;
                    if let Some((_, old)) = self.insert(record) {
                        debug!(
                            "Index({:?})：重命名 {old:?} => {:?}",
                            self.driver(),
                            self.map[&frn].1
                        );
                    }
                }
                _ => {}
            }
        }
        self.usn = usn_records.next_usn();
        Ok(())
    }
}
