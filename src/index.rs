mod find;

use fastfind_ntfs::{
    journal::JournalCursor,
    record::{UsnReasons, UsnRecordView},
    volume::Volume,
};
use log::debug;
use std::collections::HashMap;

pub use find::{FindIter, FullPath};

type V = (u64, Box<str>);

pub struct Index {
    drive: String,
    map: HashMap<u64, V>,
    cursor: JournalCursor,
}

impl Index {
    pub fn with_capacity(capacity: usize, drive: String, cursor: JournalCursor) -> Self {
        Self {
            drive,
            map: HashMap::with_capacity(capacity),
            cursor,
        }
    }

    pub fn try_from_volume(vol: &Volume, drive: String) -> anyhow::Result<Self> {
        let journal = vol.query_usn_journal()?;
        let cursor = journal.next_cursor();
        let mut index = Self::with_capacity(10_0000, drive, cursor);
        let mut iter = vol.enum_usn_data();
        while let Some(mut records) = iter.next_records()? {
            while let Some(record) = records.next_record() {
                index.insert(record);
            }
        }
        debug!("索引 {} 盘文件记录 {} 条", index.drive, index.len());
        Ok(index)
    }

    pub fn insert(&mut self, record: UsnRecordView) -> Option<V> {
        self.map.insert(
            record.file_reference_number(),
            (
                record.parent_file_reference_number(),
                record.file_name().to_string_lossy().into(),
            ),
        )
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn drive(&self) -> &str {
        &self.drive
    }

    pub fn find_iter<'a>(&'a self, sub: &'a str) -> FindIter<'a> {
        FindIter::new(self, sub)
    }

    pub fn sync(&mut self, vol: &Volume) -> anyhow::Result<()> {
        let mut iter = vol.read_usn_journal(self.cursor);
        iter.set_reason_mask(
            UsnReasons::FILE_CREATE | UsnReasons::FILE_DELETE | UsnReasons::RENAME_NEW_NAME,
        )
        // 只匹配文件关闭时的事件
        .set_only_on_close(true);

        while let Some(mut records) = iter.next_records()? {
            while let Some(record) = records.next_record() {
                if record.reasons().contains(UsnReasons::FILE_CREATE) {
                    debug!(
                        "Index({:?})：创建 {:?}",
                        self.drive(),
                        record.file_name().to_string_lossy()
                    );
                    self.insert(record);
                } else if record.reasons().contains(UsnReasons::FILE_DELETE) {
                    self.map.remove(&record.file_reference_number());
                    debug!(
                        "Index({:?})：删除 {:?}",
                        self.drive(),
                        record.file_name().to_string_lossy()
                    );
                } else if record.reasons().contains(UsnReasons::RENAME_NEW_NAME) {
                    let frn = record.file_reference_number();
                    if let Some((_, old)) = self.insert(record) {
                        debug!(
                            "Index({:?})：重命名 {old:?} => {:?}",
                            self.drive(),
                            self.map[&frn].1
                        );
                    }
                }
            }
        }

        self.cursor = iter.cursor();
        Ok(())
    }
}
