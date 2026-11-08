//! The page_sources slot: the outside addresses a feature's pages show images from or call,
//! which the dashboard's Content-Security-Policy allows beside its own. Only an outside service
//! whose terms have its media loaded straight from it, in the browser, needs one.
use crate::manifest::{Feature, Registry};

/// One outside address a feature's pages load from, as its manifest declares it.
pub struct PageSource {
    /// Permanent once shipped: `<feature>.<what>`.
    pub id: &'static str,
    /// What the pages load from it.
    pub loads: Loads,
    /// Where: `https://` and a host, nothing more.
    pub origin: &'static str,
}
/// What a page loads from an outside address.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Loads {
    /// Images it shows (`img-src`).
    Images,
    /// Calls it makes (`connect-src`).
    Calls,
}
/// One outside address: `page_source("x.media", Loads::Images, "https://media.example.com")`.
pub const fn page_source(id: &'static str, loads: Loads, origin: &'static str) -> PageSource {
    PageSource { id, loads, origin }
}

impl Registry {
    /// Every feature's page_sources, each with its feature, in the registry's order.
    pub fn page_sources(&self) -> impl Iterator<Item = (&'static Feature, &'static PageSource)> {
        self.features.iter().flat_map(|feature| {
            feature
                .page_sources
                .iter()
                .map(move |each| (*feature, each))
        })
    }

    /// The origins the pages load `loads` from, each once, as a policy lists them.
    pub fn page_origins(&self, loads: Loads) -> Vec<&'static str> {
        let mut origins: Vec<_> = self
            .page_sources()
            .filter(|(_, each)| each.loads == loads)
            .map(|(_, each)| each.origin)
            .collect();
        origins.sort_unstable();
        origins.dedup();
        origins
    }
}

/// Panics unless each of the page_sources has an id of its own, under its feature's name, and
/// an origin of `https://` and a host alone.
pub(crate) fn check(registry: &Registry) {
    let mut seen = std::collections::BTreeSet::new();
    for (feature, each) in registry.page_sources() {
        assert!(
            each.id.starts_with(&format!("{}.", feature.name)),
            "{} declares {} among its page_sources, which is not under its name",
            feature.name,
            each.id
        );
        assert!(
            seen.insert(each.id),
            "{} is declared twice among the page_sources",
            each.id
        );
        let host = each.origin.strip_prefix("https://").unwrap_or_default();
        assert!(
            !host.is_empty()
                && host.contains('.')
                && host
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-'),
            "{}'s origin {} is not https:// and a host alone",
            each.id,
            each.origin
        );
    }
}
