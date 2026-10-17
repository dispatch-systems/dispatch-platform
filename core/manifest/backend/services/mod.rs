//! The services slot: what a feature offers every other feature, through a contract core
//! defines, so a feature uses another's work without depending on it, or knowing which feature
//! does it: one feature offers somewhere to keep files, and another, bringing files in from
//! elsewhere, saves them there.
//!
//! Each service is a trait in a module of its own here, named for what it does, never for who
//! offers it. A feature offers one in its manifest's `services`; one feature at most offers
//! each. A feature using one asks for it for a DSP, and has it only while the feature offering
//! it is on for that DSP.
//!
//! Adding a service: its trait in a module here, a field for it in `Services` with a line in
//! `Services::NONE`, a `with_` to offer it and a line in `Services::offered`, and a lookup in
//! `Registry`, as `files` has.
pub mod directory;
pub mod files;

use crate::{
    Result,
    db::Store,
    manifest::{Feature, Registry},
};
use std::{future::Future, pin::Pin};

/// What a service's asynchronous answers are: work done outside the database, as a call to an
/// outside service is.
pub type Answer<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// The services a feature offers.
#[derive(Clone, Copy)]
pub struct Services {
    /// Somewhere to keep a DSP's files, in folders.
    pub files: Option<&'static dyn files::Files>,
    /// Who each ID is, for features that show the people their data names.
    pub directory: Option<&'static dyn directory::Directory>,
}
impl Services {
    /// A feature that offers none. One that offers some names each:
    /// `services: Services::NONE.with_files(&backend::service::FILES)`.
    pub const NONE: Self = Self {
        files: None,
        directory: None,
    };
    /// These, with somewhere to keep a DSP's files too.
    pub const fn with_files(mut self, files: &'static dyn files::Files) -> Self {
        self.files = Some(files);
        self
    }
    /// These, with the directory too.
    pub const fn with_directory(mut self, directory: &'static dyn directory::Directory) -> Self {
        self.directory = Some(directory);
        self
    }
    /// The names of the services offered, as the feature map lists them.
    pub fn offered(&self) -> Vec<&'static str> {
        [
            ("files", self.files.is_some()),
            ("directory", self.directory.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, offered)| offered.then_some(name))
        .collect()
    }
}

impl Registry {
    /// The feature that keeps a DSP's files, and its service, if this build has one.
    pub fn files(&self) -> Option<(&'static Feature, &'static dyn files::Files)> {
        offered(self, |services| services.files)
    }
    /// The directory, if this build has a feature that tells people apart. It answers for
    /// every DSP: that feature is one every DSP has.
    pub fn directory(&self) -> Option<&'static dyn directory::Directory> {
        offered(self, |services| services.directory).map(|(_, directory)| directory)
    }
}

/// The feature offering the service `of` picks out, and the service, if one does.
fn offered<T: ?Sized>(
    registry: &Registry,
    of: fn(&Services) -> Option<&'static T>,
) -> Option<(&'static Feature, &'static T)> {
    registry
        .features
        .iter()
        .find_map(|feature| of(&feature.services).map(|service| (*feature, service)))
}

/// The service `of` picks out for the DSP `dsp`: only while the feature offering it is on there.
pub(crate) fn for_dsp<T: ?Sized>(
    db: &Store,
    dsp: &str,
    offered: Option<(&'static Feature, &'static T)>,
) -> Result<Option<&'static T>> {
    let Some((feature, service)) = offered else {
        return Ok(None);
    };
    Ok(db
        .feature_enabled(dsp, feature.switch.id)?
        .then_some(service))
}

/// Panics unless each service is offered by one feature at most, as the registry is installed.
pub(super) fn check(features: &[&'static Feature]) {
    let mut offering = std::collections::BTreeMap::<&str, Vec<&str>>::new();
    for feature in features {
        for service in feature.services.offered() {
            offering.entry(service).or_default().push(feature.name);
        }
    }
    for (service, by) in offering {
        assert!(
            by.len() == 1,
            "{} each offer the {service} service; one feature at most may",
            by.join(" and ")
        );
    }
}
