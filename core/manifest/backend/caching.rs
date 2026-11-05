//! The read cache slots: the kinds of data a feature's reviewed writes change (`domains`), and
//! what evicts the reads it caches (`cached`).
use crate::{
    manifest::Registry,
    server::cache::{self, Cached, DataDomain},
};

impl Registry {
    /// Every domain a reviewed write may name: core's, then each feature's.
    pub fn domains(&self) -> impl Iterator<Item = DataDomain> {
        let features = self.features.iter().flat_map(|feature| feature.domains);
        cache::DOMAINS.iter().chain(features).copied()
    }
    /// What evicts each cached read: core's declarations, then each feature's.
    pub fn cached(&self) -> impl Iterator<Item = &'static Cached> {
        let features = self.features.iter().flat_map(|feature| feature.cached);
        cache::CACHED.iter().chain(features)
    }
    /// Whether a write to `domain` can change who is who: it's one a feature that names people
    /// declares, or one it publishes a collection to.
    pub fn naming_people(&self, domain: DataDomain) -> bool {
        self.features
            .iter()
            .filter(|feature| !feature.people.is_empty())
            .any(|feature| {
                feature.domains.contains(&domain)
                    || feature.keeps.iter().any(|keeper| keeper.domain() == domain)
            })
    }
}

/// Panics unless every domain is declared once, and before it is named.
pub(crate) fn check(registry: &Registry) {
    let domains: Vec<DataDomain> = registry.domains().collect();
    for (index, domain) in domains.iter().enumerate() {
        assert!(
            !domains[..index].contains(domain),
            "the {} domain is declared twice",
            domain.id()
        );
    }
    let named = registry
        .keepers()
        .map(|keeper| keeper.domain())
        .chain(registry.cached().flat_map(|cached| match cached.evicted {
            cache::Evicted::By(named)
            | cache::Evicted::ByAllBut(named)
            | cache::Evicted::ByPeopleAnd(named) => named.iter().copied(),
        }));
    for domain in named {
        assert!(
            domains.contains(&domain),
            "the {} domain is named but not declared",
            domain.id()
        );
    }
}
