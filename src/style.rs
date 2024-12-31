use core::str;
use memchr::memmem::FinderRev;
use nu_ansi_term::Style;
use std::fmt::Display;

pub struct Styled<'a> {
    style: &'a Style,
    origin: &'a str,
    finder: &'a FinderRev<'a>,
}

impl<'a> Styled<'a> {
    pub fn new(style: &'a Style, origin: &'a str, finder: &'a FinderRev<'a>) -> Self {
        Self {
            style,
            origin,
            finder,
        }
    }
}

impl<'a> Display for Styled<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lower = self.origin.to_lowercase();
        if let Some(i) = self.finder.rfind(&lower) {
            let len = self.finder.needle().len();
            return write!(
                f,
                "{}{}{}{}{}",
                &self.origin[..i],
                self.style.prefix(),
                &self.origin[i..i + len],
                self.style.suffix(),
                &self.origin[i + len..],
            );
        }

        write!(f, "{}", self.origin)
    }
}
