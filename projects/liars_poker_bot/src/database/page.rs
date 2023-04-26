use std::collections::HashMap;
use std::fmt::Debug;

use crate::istate::IStateKey;

const MAX_PAGE_LEN: usize = 999999;

/// Represents a collection of istates that are loaded into the cache.
///
/// It includes all children and parents of the `istate` it stores. The
/// `trim` variable determins where the cut happens to split istates into pages.
#[derive(Clone)]
pub struct Page<T> {
    pub istate: IStateKey,
    pub max_length: usize,
    pub cache: HashMap<IStateKey, T>,
}

impl<T> Page<T> {
    pub fn new(istate: &IStateKey, depth: &[usize]) -> Self {
        let (pgi, ml) = Page::<T>::get_key_and_max_depth(istate, depth);
        Self {
            istate: pgi,
            max_length: ml,
            cache: HashMap::new(),
        }
    }

    fn get_key_and_max_depth(istate: &IStateKey, depth: &[usize]) -> (IStateKey, usize) {
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
            true => istate.trim(total_depth),
            false => IStateKey::new(),
        };

        return (page_istate, max_length);
    }

    pub fn get_page_key(istate: &IStateKey, depth: &[usize]) -> IStateKey {
        let (pgi, _) = Page::<T>::get_key_and_max_depth(istate, depth);
        return pgi;
    }

    pub fn contains(&self, istate: &IStateKey) -> bool {
        // Parent of the current page
        if istate.len() < self.istate.len() || istate.len() > self.max_length {
            return false;
        }

        // Different parent
        let target_parent = istate.trim(self.istate.len());
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
    use crate::{cfragent::cfrnode::CFRNode, istate::IStateKey};

    use super::{Page, MAX_PAGE_LEN};

    #[test]
    fn test_page_contains() {
        let mut k = IStateKey::new();
        k.append(&[1, 0, 0, 0, 1]);
        let p: Page<CFRNode> = Page::new(&k, &[3]);

        let mut pk = IStateKey::new();
        pk.append(&[1, 0, 0]);
        assert_eq!(p.istate, pk);

        assert!(p.contains(&k));
        k.push(5);
        assert!(p.contains(&k));

        let mut dk = IStateKey::new();
        dk.append(&[1, 1, 0, 0, 1]);
        assert!(!p.contains(&dk));

        let mut sk = IStateKey::new();
        sk.append(&[1, 0]);
        let p: Page<CFRNode> = Page::new(&sk, &[3]);
        let pk = IStateKey::new(); // blank key
        assert_eq!(p.istate, pk);
        assert!(p.contains(&sk));
        assert!(!p.contains(&k)); // too long to be included
    }

    #[test]
    fn test_page_new() {
        let k = IStateKey::new();
        let p: Page<CFRNode> = Page::new(&k, &[]);
        assert_eq!(p.max_length, MAX_PAGE_LEN);
    }
}
