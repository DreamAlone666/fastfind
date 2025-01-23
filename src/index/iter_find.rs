use std::{
    collections::hash_map::Values,
    fmt::{Display, Write},
    path::MAIN_SEPARATOR,
};

use memchr::memmem::Finder;
use nu_ansi_term::Style;

use super::Index;

pub struct FullPath<'a> {
    prefix: &'a str,
    sub: &'a str,
    suffix: &'a str,
    index: &'a Index,
    parent_frn: u64,
    style: Option<&'a Style>,
}

impl<'a> FullPath<'a> {
    pub fn style(&mut self, style: &'a Style) {
        self.style = Some(style);
    }
}

impl Display for FullPath<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<&str> = Vec::new();
        let mut frn = self.parent_frn;
        while let Some((parent_frn, name)) = self.index.map.get(&frn) {
            parts.push(name);
            frn = *parent_frn;
        }

        parts.reverse();
        f.write_str(self.index.driver())?;
        f.write_char(MAIN_SEPARATOR)?;
        for part in parts {
            f.write_str(part)?;
            f.write_char(MAIN_SEPARATOR)?;
        }
        f.write_str(self.prefix)?;
        if let Some(s) = self.style {
            write!(f, "{}", s.paint(self.sub))?;
        } else {
            f.write_str(self.sub)?;
        }
        f.write_str(self.suffix)
    }
}

pub struct IterFind<'a> {
    index: &'a Index,
    sub: &'a str,
    finder: Finder<'a>,
    values: Values<'a, u64, (u64, Box<str>)>,
}

impl<'a> IterFind<'a> {
    pub fn new(index: &'a Index, sub: &'a str) -> Self {
        Self {
            index,
            sub,
            finder: Finder::new(&sub.to_lowercase()).into_owned(),
            values: index.map.values(),
        }
    }
}

impl<'a> Iterator for IterFind<'a> {
    type Item = FullPath<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (parent_frn, name) = self.values.next()?;
            if let Some(mid) = self.finder.find(name.to_lowercase().as_bytes()) {
                return Some(FullPath {
                    prefix: &name[..mid],
                    sub: &name[mid..(mid + self.sub.len())],
                    suffix: &name[(mid + self.sub.len())..],
                    index: self.index,
                    parent_frn: *parent_frn,
                    style: None,
                });
            }
        }
    }
}
