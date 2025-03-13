use memchr::memmem::Finder;
use std::{
    collections::hash_map::Values,
    fmt::Display,
    path::{Path, MAIN_SEPARATOR_STR},
};

use super::Index;

pub struct FullPath {
    pub inner: String,
    sub_start: usize,
    sub_end: usize,
}

impl FullPath {
    /// 将路径按照查找时的关键词分割为三个部分，
    /// 其中中间的部分为匹配到的关键词。
    pub fn split(&self) -> (&str, &str, &str) {
        (
            &self.inner[..self.sub_start],
            &self.inner[self.sub_start..self.sub_end],
            &self.inner[self.sub_end..],
        )
    }
}

impl Display for FullPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl AsRef<Path> for FullPath {
    fn as_ref(&self) -> &Path {
        Path::new(&self.inner)
    }
}

pub struct FindIter<'a> {
    index: &'a Index,
    sub: &'a str,
    finder: Finder<'a>,
    values: Values<'a, u64, (u64, Box<str>)>,
}

impl<'a> FindIter<'a> {
    pub fn new(index: &'a Index, sub: &'a str) -> Self {
        Self {
            index,
            sub,
            finder: Finder::new(&sub.to_lowercase()).into_owned(),
            values: index.map.values(),
        }
    }
}

impl<'a> Iterator for FindIter<'a> {
    type Item = FullPath;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (parent_frn, name) = self.values.next()?;
            if let Some(mid) = self.finder.find(name.to_lowercase().as_bytes()) {
                let mut parts: Vec<&str> = Vec::new();
                let mut frn = *parent_frn;
                while let Some((parent_frn, name)) = self.index.map.get(&frn) {
                    parts.push(name);
                    frn = *parent_frn;
                }
                parts.push(&self.index.driver);
                parts.reverse();
                parts.push(&name);

                let path = parts.join(MAIN_SEPARATOR_STR);
                let sub_start = path.len() - name.len() + mid;
                let sub_end = sub_start + self.sub.len();

                return Some(FullPath {
                    inner: path,
                    sub_start,
                    sub_end,
                });
            }
        }
    }
}
