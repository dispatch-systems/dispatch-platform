//! The switch slots: whether every DSP has a feature or the platform owner switches it per DSP
//! (`switch`), and the parts of its page switched on their own (`subfeatures`).
use crate::manifest::{Permission, Registry};

/// A feature on the DSPs page: optional, off for every DSP until the platform owner switches
/// it on, or mandatory, which every DSP has and nobody switches.
#[derive(Clone, Copy)]
pub struct Switch {
    /// Its catalog id. Permanent: each DSP's switch is stored under it.
    pub id: &'static str,
    pub label: &'static str,
    /// The capabilities an enabled connection must provide while the page is on.
    pub requires: &'static [&'static str],
    /// Whether every DSP has it, with no switch to turn it off.
    pub mandatory: bool,
    /// The features it grew from: new to a platform that ran before, it starts on, once, for
    /// each DSP with one of them on.
    pub starts_on_with: &'static [&'static str],
}
/// A feature the platform owner switches on per DSP: `optional("dvic", "DVIC", &["dvic"])`.
/// It needs a connection providing each capability it `requires`.
pub const fn optional(
    id: &'static str,
    label: &'static str,
    requires: &'static [&'static str],
) -> Switch {
    Switch {
        id,
        label,
        requires,
        mandatory: false,
        starts_on_with: &[],
    }
}
/// A feature every DSP has, with no switch: `mandatory("home", "Home Page")`. It needs no
/// connection, since a DSP without any has it too.
pub const fn mandatory(id: &'static str, label: &'static str) -> Switch {
    Switch {
        id,
        label,
        requires: &[],
        mandatory: true,
        starts_on_with: &[],
    }
}
impl Switch {
    /// Starts on, once, for each DSP with one of `features` on, when it is new to a platform
    /// that ran before: `optional(..).starts_on_with(&["older_feature"])`.
    pub const fn starts_on_with(self, features: &'static [&'static str]) -> Self {
        Self {
            starts_on_with: features,
            ..self
        }
    }
}
/// What a manifest that hasn't chosen has; the registry refuses it.
pub(crate) const UNDECLARED: Switch = Switch {
    id: "",
    label: "",
    requires: &[],
    mandatory: false,
    starts_on_with: &[],
};
/// A part of a feature's page, switched on its own inside the page: one of its tabs, or any
/// other part, such as uploading files. It exists while it and its page are on, and its
/// permissions with it. A part is optional unless `.mandatory()`, which only a mandatory
/// feature's parts may be. An optional part of an optional feature starts on, so switching
/// the feature on shows all of it; one of a mandatory feature starts off, as a new feature does.
pub struct Sub {
    /// Its catalog id. Permanent: each DSP's switch is stored under it.
    pub id: &'static str,
    pub label: &'static str,
    /// Whether it is one of the page's tabs. A page keeps one of its tabs on at least.
    pub tab: bool,
    /// The permissions it owns, listed on the role sheet under its page's.
    pub permissions: &'static [Permission],
    /// The capabilities an enabled connection must provide while it is on, beyond its page's.
    pub requires: &'static [&'static str],
    /// Whether every DSP with its page has it, with no switch to turn it off.
    pub mandatory: bool,
}
/// A tab of a feature's page: `tab("dvic.week", "Week")`.
pub const fn tab(id: &'static str, label: &'static str) -> Sub {
    Sub {
        id,
        label,
        tab: true,
        permissions: &[],
        requires: &[],
        mandatory: false,
    }
}
/// A part of a feature's page that isn't a tab: `sub("documents.uploads", "Uploads")`.
pub const fn sub(id: &'static str, label: &'static str) -> Sub {
    Sub {
        tab: false,
        ..tab(id, label)
    }
}
impl Sub {
    pub const fn permissions(self, permissions: &'static [Permission]) -> Self {
        Self {
            permissions,
            ..self
        }
    }
    /// What it needs a connection to provide while it is on, as `optional` names a feature's.
    pub const fn requires(self, requires: &'static [&'static str]) -> Self {
        Self { requires, ..self }
    }
    /// Every DSP with its page has it: only a mandatory feature's part may be.
    pub const fn mandatory(self) -> Self {
        Self {
            mandatory: true,
            ..self
        }
    }
}

/// Panics unless every feature says whether it is mandatory or optional, and only a mandatory
/// feature's parts are mandatory, with no connection required.
pub(crate) fn check(registry: &Registry) {
    for feature in registry.features {
        let switch = feature.switch;
        assert!(
            !switch.id.is_empty() && !switch.label.is_empty(),
            "{} says neither whether it is mandatory nor optional: give it \
             `switch: optional(..)` or `switch: mandatory(..)`",
            feature.name
        );
        for sub in feature.subfeatures {
            assert!(
                switch.mandatory || !sub.mandatory,
                "{} is optional, so its parts are too: {} can't be mandatory",
                feature.name,
                sub.id
            );
            assert!(
                !sub.mandatory || sub.requires.is_empty(),
                "{} is mandatory, so every DSP has it, connected or not: it can't require \
                 a connection",
                sub.id
            );
        }
    }
}
