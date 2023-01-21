use std::collections::HashMap;
use std::fmt::Debug;

use crate::cfragent::CFRNode;

/// Magic number dependant on the format of the istate. Should choose a number that has a meaningful break.
/// For example:
///     ASTDJDQDKDKH3C|ASTSKSAC|9C9HTDQC|
/// Is 33 characters. This creates a clean break between rounds

/// Determines where the page-breaks are for a euchre istate
/// For example:
///     9CTCJCKCKSKH3C|ASTSKSAC|9C9HTDQC|JD9DTCJH|JSKCQHQD|KDADXXXX|
///     |    A    | B |   1    |     2  |    3   |   4    |    5   |
/// Where:
///     A) 11 characters for the hand
///     b) 5 characters for the flip and the call
///     1-5) 9 characters for cards that have been played
///
/// A has 304 possible direct children:
///     19 cards * 4 suits * 4 possible calls = 304
///
/// B has X possible direct children:
///     18 Choose 5 = 8568
///
/// For B-5, there are ~27M ways the game can be played out. Implies that 1-5 have 90k end states.
///     27M / 304 = 90k
pub(super) const EUCHRE_PAGE_TRIM: usize = 15;
// Need to eventually implement another cut, can't have all nodes loaded to ""

/// Represents a collection of istates that are loaded into the cache.
///
/// It includes all children and parents of the `istate` it stores. The
/// `trim` variable determins where the cut happens to split istates into pages.
pub(super) struct Page {
    pub istate: String,
    depth: usize,
    pub cache: HashMap<String, CFRNode>,
}

impl Page {
    pub fn new(istate: &str, depth: usize) -> Self {
        let page_istate = match istate.len() > depth {
            true => istate[0..depth].to_string(),
            false => "".to_string(),
        };

        Self {
            istate: page_istate,
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

        if self.istate == "" {
            return istate.len() < self.depth;
        }
        return true;
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
        let p = Page::new("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD", 15);
        assert_eq!(p.istate, "AC9HJHQHAHKH3C|");

        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD"));
        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JDAD"));
        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JDADKCAH|XXXXXXXXXXXXXXXXXXXXXXX"));
        assert!(!p.contains("XXXXXXXXXXXXXX|AS10SKSAC|9CQHQDJS|JDADKCAH|"));
        assert!(!p.contains("AC9HJHQHAHKH"));

        let p = Page::new("AC9HJHQHA", 15);
        assert_eq!(p.istate, "");
        assert!(!p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD"))
    }
}
