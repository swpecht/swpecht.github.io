use std::collections::HashMap;
use std::fmt::Debug;

/// Magic number dependant on the format of the istate. Should choose a number that has a meaningful break.
/// For example:
///     ASTDJDQDKDKH3C|ASTSKSAC|9C9HTDQC|
/// Is 33 characters. This creates a clean break between rounds

/// Determines where the page-breaks are for a euchre istate
/// For example:
///     9CTCJCKCKS|KH|PPPPPPCP|3H|ASTSKSAC|9C9HTDQC|JD9DTCJH|JSKCQHQD|KDADXXXX|
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
/// B has 8568 possible direct children:
///     18 Choose 5 = 8568
///
/// For B-5, there are ~27M ways the game can be played out. Implies that 1-5 have 90k end states.
///     27M / 304 = 90k
pub(super) const EUCHRE_PAGE_TRIM: &[usize] = &[26, 2];
// Need to eventually implement another cut, can't have all nodes loaded to ""

const MAX_PAGE_LEN: usize = 999999;

/// Represents a collection of istates that are loaded into the cache.
///
/// It includes all children and parents of the `istate` it stores. The
/// `trim` variable determins where the cut happens to split istates into pages.
#[derive(Clone)]
pub struct Page<T> {
    pub istate: String,
    pub max_length: usize,
    pub cache: HashMap<String, T>,
}

impl<T> Page<T> {
    pub fn new(istate: &str, depth: &[usize]) -> Self {
        let mut total_depth = 0;
        let mut max_length = total_depth;
        for d in depth {
            if total_depth + d < istate.len() {
                total_depth += d;
                max_length = total_depth;
            } else {
                max_length += d;
                break;
            }
        }

        if max_length == total_depth {
            max_length = MAX_PAGE_LEN;
        }

        let page_istate = match istate.len() > total_depth {
            true => istate[0..total_depth].to_string(),
            false => "".to_string(),
        };

        Self {
            istate: page_istate,
            max_length: max_length,
            cache: HashMap::new(),
        }
    }

    pub fn contains(&self, istate: &str) -> bool {
        // Parent of the current page
        if istate.len() < self.istate.len() || istate.len() > self.max_length {
            return false;
        }

        // Different parent
        let target_parent = &istate[0..self.istate.len()];
        if target_parent != self.istate {
            return false;
        }

        return true;
    }
}

impl<T> Debug for Page<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("istate", &self.istate)
            .field("max_length", &self.max_length)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::cfragent::CFRNode;

    use super::{Page, MAX_PAGE_LEN};

    #[test]
    fn test_page_contains() {
        let p: Page<CFRNode> = Page::new("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD", &[15]);
        assert_eq!(p.istate, "AC9HJHQHAHKH3C|");

        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD"));
        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JDAD"));
        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JDADKCAH|XXXXXXXXXXXXXXXXXXXXXXX"));
        assert!(!p.contains("XXXXXXXXXXXXXX|AS10SKSAC|9CQHQDJS|JDADKCAH|"));
        assert!(!p.contains("AC9HJHQHAHKH"));

        let p: Page<CFRNode> = Page::new("AC9HJHQHA", &[15]);
        assert_eq!(p.istate, "");
        assert!(!p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD"));

        let p: Page<CFRNode> = Page::new("AC9HJHQHAHKH3C|ASTSKSAC|9CQHQDJS|JD", &[15, 9]);
        assert_eq!(p.istate, "AC9HJHQHAHKH3C|ASTSKSAC|");

        let p: Page<CFRNode> = Page::new("test", &[15, 9]);
        assert!(p.contains("test"));
    }

    #[test]
    fn test_page_new() {
        let p: Page<CFRNode> = Page::new("", &[]);
        assert_eq!(p.max_length, MAX_PAGE_LEN);
    }
}