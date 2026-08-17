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
    lowered: String,
}

impl<'a> FindIter<'a> {
    pub fn new(index: &'a Index, sub: &'a str) -> Self {
        Self {
            index,
            sub,
            finder: Finder::new(&sub.to_ascii_lowercase()).into_owned(),
            values: index.map.values(),
            lowered: String::new(),
        }
    }
}

impl Iterator for FindIter<'_> {
    type Item = FullPath;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (parent_frn, name) = self.values.next()?;
            self.lowered.clear();
            self.lowered.push_str(name);
            self.lowered.make_ascii_lowercase();

            if let Some(mid) = self.finder.find(self.lowered.as_bytes()) {
                let mut parts: Vec<&str> = Vec::new();
                let mut frn = *parent_frn;
                while let Some((parent_frn, name)) = self.index.map.get(&frn) {
                    parts.push(name);
                    frn = *parent_frn;
                }
                parts.push(&self.index.driver);
                parts.reverse();
                parts.push(name);

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

#[cfg(test)]
mod tests {
    use super::{FullPath, Index};
    use std::collections::HashMap;

    fn index_with_file(name: &str) -> Index {
        let mut map: HashMap<u64, (u64, Box<str>)> = HashMap::new();
        map.insert(1, (0, "dir".into()));
        map.insert(2, (1, name.into()));

        Index {
            driver: "C:".into(),
            map,
            usn: 0,
        }
    }

    fn find_one(name: &str, query: &str) -> FullPath {
        index_with_file(name)
            .find_iter(query)
            .next()
            .expect("expected a matching path")
    }

    #[test]
    fn ascii_case_insensitive_matching() {
        for (name, query, expected) in [
            ("FOO.txt", "foo", "FOO"),
            ("foo.txt", "FOO", "foo"),
            ("FoO.txt", "fOo", "FoO"),
        ] {
            let path = find_one(name, query);
            assert_eq!(path.split().1, expected);
        }
    }

    #[test]
    fn ascii_lowercase_preserves_offset_after_unicode_case_expansion() {
        // U+0130 (İ) lowercases to `i` + U+0307 under Unicode case mapping,
        // changing the byte offset of the following ASCII match.
        let path = find_one("aİFOO.txt", "foo");
        let (prefix, matched, suffix) = path.split();

        assert!(prefix.ends_with("aİ"));
        assert_eq!(matched, "FOO");
        assert_eq!(suffix, ".txt");
    }

    #[test]
    fn non_ascii_case_is_not_folded() {
        let index = index_with_file("É.txt");
        assert!(index.find_iter("é").next().is_none());

        let index = index_with_file("é.txt");
        assert!(index.find_iter("é").next().is_some());
    }

    #[test]
    fn unmatched_query_returns_no_path() {
        let index = index_with_file("bar.txt");
        assert!(index.find_iter("foo").next().is_none());
    }
}
