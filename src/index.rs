use crate::ntfs::USNRecord;
use std::collections::HashMap;

pub struct Index(HashMap<u64, (u64, String)>);

impl Index {
    pub fn with_capacity(capacity: usize) -> Self {
        Self(HashMap::with_capacity(capacity))
    }

    pub fn set(&mut self, record: USNRecord) {
        self.0
            .insert(record.frn, (record.parent_frn, record.filename));
    }

    pub fn full_name(&self, mut frn: u64) -> String {
        let mut res = String::new();
        while let Some((parent_frn, name)) = self.0.get(&frn) {
            res = r"\".to_string() + name + &res;
            frn = *parent_frn;
        }
        res
    }
}

impl IntoIterator for Index {
    type Item = <HashMap<u64, (u64, String)> as IntoIterator>::Item;
    type IntoIter = <HashMap<u64, (u64, String)> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Index {
    type Item = <&'a HashMap<u64, (u64, String)> as IntoIterator>::Item;
    type IntoIter = <&'a HashMap<u64, (u64, String)> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
