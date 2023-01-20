use std::collections::HashMap;
use std::fmt::Debug;

use crate::cfragent::CFRNode;

/// Magic number dependant on the format of the istate. Should choose a number that has a meaningful break.
/// For example:
///     ASTDJDQDKDKH3C|ASTSKSAC|9C9HTDQC|
/// Is 33 characters. This creates a clean break between rounds
pub(super) const PAGE_TRIM: usize = 33;

/// Represents a collection of istates that are loaded into the cache.
///
/// It includes all children and parents of the `istate` it stores. The
/// `trim` variable determins how large the page is. It determins how many
/// istate characters must math
pub(super) struct Page {
    pub istate: String,
    depth: usize,
    pub cache: HashMap<String, CFRNode>,
}

impl Page {
    pub fn new(istate: &str, depth: usize) -> Self {
        Self {
            istate: istate.to_string(),
            depth: depth,
            cache: HashMap::new(),
        }
    }

    pub fn contains(&self, istate: &str) -> bool {
        // Parent of the current page
        if istate.len() < self.istate.len() {
            return false;
        }

        // Different parent
        let target_parent = &istate[0..self.istate.len()];
        if target_parent != self.istate {
            return false;
        }

        return istate.len() <= self.istate.len() + self.depth;
    }
}

impl Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("istate", &self.istate)
            .field("depth", &self.depth)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::Page;

    #[test]
    fn test_page_contains() {
        let p = Page::new("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD", 6);

        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD"));
        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JDAD"));
        assert!(!p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JDADKCAH|"));
        assert!(!p.contains("XXXXXXXXXXXXXX|AS10SKSAC|9CQHQDJS|JDADKCAH|"));

        let istate = "9CTCJCKCKSKH3C|ASTSKSAC|9C9HTDQC|JD9DTCJH|JSJCQHQD|AD";
        let excess = istate.len() % 33;
        let page_istate = &istate[0..istate.len() - excess];
        let p = Page::new(page_istate, 33);
        assert!(p.contains("9CTCJCKCKSKH3C|ASTSKSAC|9C9HTDQC|JD9DTCJH|JSKCQHQD|KDAD"));
    }
}
