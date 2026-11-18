use crate::Span;
use std::{cell::RefCell, collections::BTreeSet};

const MAX_OPERATIONS: usize = 50_000;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CleanupSite {
    pub(crate) kind: &'static str,
    pub(crate) origin: Span,
    pub(crate) contract: Vec<String>,
    pub(crate) completion: String,
    pub(crate) closed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ProtocolSite {
    pub(crate) family: &'static str,
    pub(crate) origin: Span,
    pub(crate) contract: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Journal {
    pub(crate) sites: BTreeSet<CleanupSite>,
    pub(crate) protocols: BTreeSet<ProtocolSite>,
    pub(crate) truncated: bool,
}

impl Journal {
    fn push(&mut self, site: CleanupSite) {
        if self.sites.len() + self.protocols.len() < MAX_OPERATIONS {
            self.sites.insert(site);
        } else {
            self.truncated = true;
        }
    }

    fn push_protocol(&mut self, site: ProtocolSite) {
        if self.sites.len() + self.protocols.len() < MAX_OPERATIONS {
            self.protocols.insert(site);
        } else {
            self.truncated = true;
        }
    }

    pub(crate) fn append(&mut self, other: &Self) {
        self.truncated |= other.truncated;
        for site in &other.sites {
            self.push(site.clone());
        }
        for site in &other.protocols {
            self.push_protocol(site.clone());
        }
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Journal>> = const { RefCell::new(None) };
}

pub(crate) fn collect<T>(enabled: bool, run: impl FnOnce() -> T) -> (T, Journal) {
    struct Restore(Option<Journal>);

    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.replace(self.0.take()));
        }
    }

    let _restore = Restore(ACTIVE.with(|active| active.replace(enabled.then(Journal::default))));
    let result = run();
    let journal = ACTIVE.with(|active| active.take().unwrap_or_default());
    (result, journal)
}

pub(crate) fn record(site: impl FnOnce() -> CleanupSite) {
    ACTIVE.with(|active| {
        if let Some(journal) = active.borrow_mut().as_mut() {
            journal.push(site());
        }
    });
}

pub(crate) fn record_protocol(family: &'static str, origin: Span, contract: Vec<String>) {
    ACTIVE.with(|active| {
        if let Some(journal) = active.borrow_mut().as_mut() {
            journal.push_protocol(ProtocolSite {
                family,
                origin,
                contract,
            });
        }
    });
}
