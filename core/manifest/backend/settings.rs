//! The settings slot: what a DSP's members set, each feature's in sections of the Settings
//! page, which Settings draws from these declarations alone. A feature declares its sections
//! (`settings`), each with its fields, and reads their values in its backend with
//! `crate::settings::read`; core keeps them, checks what's saved and records it in the log.
//! The pages several owners share, and core's own sections, are declared here too.
use crate::{
    Result,
    accounts::Context,
    db::Store,
    manifest::{Feature, Registry},
    tenancy::roles,
};
use serde_json::Value;
use std::collections::BTreeSet;

/// The pages several owners' sections share, core's own: the member's own in Personal, the
/// DSP's in DSP. A feature's own page is in Features.
pub const SHARED_PAGES: &[SharedPage] = &[
    shared("general", "Profile", Group::Personal, 10),
    shared("security", "Security", Group::Personal, 20),
    shared("theme", "Theme", Group::Personal, 90),
    shared("display", "Display", Group::Personal, 95),
    shared("dsp", "DSP profile", Group::Dsp, 10),
    shared("connections", "Connections", Group::Dsp, 30),
    shared("collectors", "Collectors", Group::Dsp, 40),
    shared("data", "Data", Group::Dsp, 50),
];

/// Core's own sections, each declared by the core part whose screen draws it.
const CORE_SECTIONS: &[&Section] = &[
    &crate::accounts::settings::PROFILE,
    &crate::accounts::settings::SECURITY,
    &crate::accounts::settings::THEME,
    &crate::collection::settings::CONNECTIONS,
    &crate::collection::settings::COLLECTORS,
];

/// A part of a Settings page: fields saved together, under one permission.
pub struct Section {
    /// Permanent: `<feature>.<what>` (a core part's name for core's). Its values are kept
    /// under it.
    pub id: &'static str,
    pub label: &'static str,
    /// The page it's on: its feature's own (`Page::Own`), or one of the shared pages.
    pub page: Page,
    /// Who sees and changes it: a permission its feature declares (core's own for core's),
    /// or `roles::ACCESS` for every member.
    pub permission: &'static str,
    /// One value for the DSP, or each member's own.
    pub scope: Scope,
    pub fields: &'static [Field],
    /// More words search finds it by, as "password" finds Security.
    pub keywords: &'static [&'static str],
    /// More words search finds it by that depend on the DSP, listed as a member reads it, as
    /// Connections is found by the accounts the DSP can connect.
    pub found: Option<Found>,
    /// Drawn by its owner's own component, registered in the frontend's `settingsSections`,
    /// for what isn't a value: a sign-in flow, a list of channels. Only it may have no fields.
    pub custom: bool,
    /// Where its values live when not in core's store, as the DSP profile's are in its own
    /// tables. Only a section of the DSP's own has one.
    pub kept: Option<&'static dyn Kept>,
    /// The words to confirm before a save that loses something; it answers none to save
    /// without asking.
    pub warn: Option<Warn>,
    /// What else a save does, in the same write, as retiming schedules after a timezone
    /// changes.
    pub saved: Option<OnSave>,
    /// The part of its feature (`x.what`) it goes with: off or hidden with it.
    pub part: Option<&'static str>,
    /// The collection of its feature's it goes with, by its schedule name: drawn with it on
    /// the Collectors page, under the permission that sets when it runs.
    pub collection: Option<&'static str>,
    /// Its place on its page, lowest first; two at one place go by id.
    pub order: u16,
}
/// A section of the DSP's, on its feature's own page, with nothing else said:
/// `section("x.what", "What", "x.manage", &[..]).on("data").order(10)`.
pub const fn section(
    id: &'static str,
    label: &'static str,
    permission: &'static str,
    fields: &'static [Field],
) -> Section {
    Section {
        id,
        label,
        page: Page::Own,
        permission,
        scope: Scope::Dsp,
        fields,
        keywords: &[],
        found: None,
        custom: false,
        kept: None,
        warn: None,
        saved: None,
        part: None,
        collection: None,
        order: 0,
    }
}
impl Section {
    /// On the shared page `page`, rather than its feature's own.
    pub const fn on(self, page: &'static str) -> Self {
        Self {
            page: Page::Shared(page),
            ..self
        }
    }
    /// Each member's own, rather than the DSP's.
    pub const fn member(self) -> Self {
        Self {
            scope: Scope::Member,
            ..self
        }
    }
    pub const fn keywords(self, keywords: &'static [&'static str]) -> Self {
        Self { keywords, ..self }
    }
    pub const fn found(self, found: Found) -> Self {
        Self {
            found: Some(found),
            ..self
        }
    }
    /// Drawn by its owner's component rather than by Settings.
    pub const fn custom(self) -> Self {
        Self {
            custom: true,
            ..self
        }
    }
    pub const fn kept(self, kept: &'static dyn Kept) -> Self {
        Self {
            kept: Some(kept),
            ..self
        }
    }
    pub const fn warn(self, warn: Warn) -> Self {
        Self {
            warn: Some(warn),
            ..self
        }
    }
    pub const fn saved(self, saved: OnSave) -> Self {
        Self {
            saved: Some(saved),
            ..self
        }
    }
    pub const fn part(self, part: &'static str) -> Self {
        Self {
            part: Some(part),
            ..self
        }
    }
    /// With the collection `schedule` names, on the Collectors page, rather than on its
    /// feature's own page.
    pub const fn collection(self, schedule: &'static str) -> Self {
        Self {
            page: Page::Shared("collectors"),
            collection: Some(schedule),
            ..self
        }
    }
    pub const fn order(self, order: u16) -> Self {
        Self { order, ..self }
    }
}

/// The page a section is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// Its feature's own, in Features: its switch's id with `-` for `_`, and its label.
    Own,
    /// One of `SHARED_PAGES`, by id.
    Shared(&'static str),
}
/// Whose a section's values are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// One for the DSP, which the log records each change of.
    Dsp,
    /// Each member's own, which the log doesn't record.
    Member,
}
/// A page several owners' sections share.
pub struct SharedPage {
    /// Permanent: links name it (`?tab=`).
    pub id: &'static str,
    pub label: &'static str,
    pub group: Group,
    /// Its place in its group, lowest first.
    pub order: u16,
}
const fn shared(id: &'static str, label: &'static str, group: Group, order: u16) -> SharedPage {
    SharedPage {
        id,
        label,
        group,
        order,
    }
}
/// The groups of Settings' pages, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// The member's own.
    Personal,
    /// The DSP's, which several owners share.
    Dsp,
    /// Each feature's own page.
    Features,
}

/// Where a section's values live when not in core's store. Core still keeps its revision,
/// and checks, merges and records each save as it does any section's. Each is handed the DSP
/// the member's view authorized, and reads and writes that DSP's data alone.
pub trait Kept: Sync {
    /// Its values as they are now, by field id; a field it leaves out reads as its default.
    /// Settings loads it each time it's read, so it's one indexed query. One that fails leaves
    /// just its section out of the member's Settings, and fails a save of it.
    fn load(&self, db: &Store, dsp: &str) -> Result<Value>;
    /// Keeps `values`, every field's, in the same write as the save, before its `saved` and
    /// its record in the log; it records nothing in the log itself.
    fn keep(&self, db: &Store, dsp: &str, actor: &str, values: &Value) -> Result<()>;
}
/// The words to confirm a save with, given the values before and after it; none to save
/// without asking. It reads only `dsp`'s data, the DSP being saved.
pub type Warn = fn(&Store, dsp: &str, before: &Value, after: &Value) -> Result<Option<String>>;
/// What else a save does, in the same write, given the values it saved: to `dsp`'s data alone.
pub type OnSave = fn(&Store, dsp: &str, values: &Value) -> Result<()>;
/// More words a section is found by, from what the member's own DSP has, which `c` is: its
/// features, and the member's role and permissions, never who they are, as one answer serves
/// every member with the same access; and never the DSP's stored data, as it runs for each
/// member's Settings.
pub type Found = fn(c: &Context) -> Vec<String>;
/// The options a field lists as it's read, each a value and its label, as a DSP's
/// departments: `dsp`'s alone. It runs once for each member's Settings that shows a section
/// offering it, however many do, and for each save of one, so it's one indexed query, never a
/// scan of collected data. One that fails leaves just the sections offering it out of the
/// member's Settings, and fails a save of them.
pub type Listed = fn(&Store, dsp: &str) -> Result<Vec<(String, String)>>;

/// One value of a section.
#[derive(Clone, Copy)]
pub struct Field {
    /// Its name in the section's values, a serde name of what its feature reads them as:
    /// snake_case, permanent once shipped.
    pub id: &'static str,
    pub label: &'static str,
    pub kind: Kind,
}
/// What a field holds, with its default.
#[derive(Clone, Copy)]
pub enum Kind {
    /// On or off.
    Toggle { default: bool },
    /// One of fixed options, each a value and its label.
    Choice {
        options: &'static [(&'static str, &'static str)],
        default: &'static str,
    },
    /// One of the options its feature lists as it's read, or with `none` (its label), none.
    Pick {
        options: Listed,
        default: Option<&'static str>,
        none: Option<&'static str>,
    },
    /// Several of its options, in their order, or in the member's when `ordered`.
    Many {
        options: Options,
        default: &'static [&'static str],
        ordered: bool,
    },
    /// A whole number from `min` to `max` in `unit`, with `presets` offered, or with `none`
    /// (its label), none.
    Number {
        min: i64,
        max: i64,
        default: Option<i64>,
        unit: Option<&'static str>,
        presets: &'static [(i64, &'static str)],
        none: Option<&'static str>,
    },
    /// `min` to `max` characters; empty only when its default is, and only until it's set. A
    /// `code` is uppercase letters and digits, and a `locked` one can't change once set.
    Text {
        min: usize,
        max: usize,
        default: &'static str,
        code: bool,
        locked: bool,
    },
    /// A time of day, `HH:MM`.
    Time { default: &'static str },
    /// A timezone, by its IANA name.
    Timezone { default: &'static str },
}
/// The options of a `many`: fixed, or listed as it's read.
#[derive(Clone, Copy)]
pub enum Options {
    Fixed(&'static [(&'static str, &'static str)]),
    Listed(Listed),
}

const fn field(id: &'static str, label: &'static str, kind: Kind) -> Field {
    Field { id, label, kind }
}
pub const fn toggle(id: &'static str, label: &'static str, default: bool) -> Field {
    field(id, label, Kind::Toggle { default })
}
pub const fn choice(
    id: &'static str,
    label: &'static str,
    options: &'static [(&'static str, &'static str)],
    default: &'static str,
) -> Field {
    field(id, label, Kind::Choice { options, default })
}
/// One of the options `options` lists as it's read; a default of none needs `.none(..)`.
pub const fn pick(
    id: &'static str,
    label: &'static str,
    options: Listed,
    default: Option<&'static str>,
) -> Field {
    let none = None;
    field(
        id,
        label,
        Kind::Pick {
            options,
            default,
            none,
        },
    )
}
pub const fn many(
    id: &'static str,
    label: &'static str,
    options: Options,
    default: &'static [&'static str],
) -> Field {
    let ordered = false;
    field(
        id,
        label,
        Kind::Many {
            options,
            default,
            ordered,
        },
    )
}
/// A whole number from `min` to `max`; a default of none needs `.none(..)`.
pub const fn number(
    id: &'static str,
    label: &'static str,
    min: i64,
    max: i64,
    default: Option<i64>,
) -> Field {
    let kind = Kind::Number {
        min,
        max,
        default,
        unit: None,
        presets: &[],
        none: None,
    };
    field(id, label, kind)
}
pub const fn text(
    id: &'static str,
    label: &'static str,
    min: usize,
    max: usize,
    default: &'static str,
) -> Field {
    let kind = Kind::Text {
        min,
        max,
        default,
        code: false,
        locked: false,
    };
    field(id, label, kind)
}
pub const fn time(id: &'static str, label: &'static str, default: &'static str) -> Field {
    field(id, label, Kind::Time { default })
}
pub const fn timezone(id: &'static str, label: &'static str, default: &'static str) -> Field {
    field(id, label, Kind::Timezone { default })
}
impl Field {
    /// A number's unit: "days".
    pub const fn unit(self, unit: &'static str) -> Self {
        match self.kind {
            Kind::Number {
                min,
                max,
                default,
                presets,
                none,
                ..
            } => {
                let unit = Some(unit);
                let kind = Kind::Number {
                    min,
                    max,
                    default,
                    unit,
                    presets,
                    none,
                };
                Self { kind, ..self }
            }
            _ => panic!("only a number has a unit"),
        }
    }
    /// The values a number offers, each with its label: `&[(30, "30 days")]`.
    pub const fn presets(self, presets: &'static [(i64, &'static str)]) -> Self {
        match self.kind {
            Kind::Number {
                min,
                max,
                default,
                unit,
                none,
                ..
            } => {
                let kind = Kind::Number {
                    min,
                    max,
                    default,
                    unit,
                    presets,
                    none,
                };
                Self { kind, ..self }
            }
            _ => panic!("only a number has presets"),
        }
    }
    /// A number or a pick may be none, which reads as `label`: "Every day".
    pub const fn none(self, label: &'static str) -> Self {
        let none = Some(label);
        match self.kind {
            Kind::Number {
                min,
                max,
                default,
                unit,
                presets,
                ..
            } => {
                let kind = Kind::Number {
                    min,
                    max,
                    default,
                    unit,
                    presets,
                    none,
                };
                Self { kind, ..self }
            }
            Kind::Pick {
                options, default, ..
            } => {
                let kind = Kind::Pick {
                    options,
                    default,
                    none,
                };
                Self { kind, ..self }
            }
            _ => panic!("only a number or a pick may be none"),
        }
    }
    /// A `many` whose order is the member's.
    pub const fn ordered(self) -> Self {
        match self.kind {
            Kind::Many {
                options, default, ..
            } => {
                let ordered = true;
                let kind = Kind::Many {
                    options,
                    default,
                    ordered,
                };
                Self { kind, ..self }
            }
            _ => panic!("only a many is ordered"),
        }
    }
    /// A text of uppercase letters and digits, as a short code.
    pub const fn code(self) -> Self {
        match self.kind {
            Kind::Text {
                min,
                max,
                default,
                locked,
                ..
            } => {
                let code = true;
                let kind = Kind::Text {
                    min,
                    max,
                    default,
                    code,
                    locked,
                };
                Self { kind, ..self }
            }
            _ => panic!("only a text is a code"),
        }
    }
    /// A text that can't change once set.
    pub const fn locked(self) -> Self {
        match self.kind {
            Kind::Text {
                min,
                max,
                default,
                code,
                ..
            } => {
                let locked = true;
                let kind = Kind::Text {
                    min,
                    max,
                    default,
                    code,
                    locked,
                };
                Self { kind, ..self }
            }
            _ => panic!("only a text is locked"),
        }
    }
}

impl Registry {
    /// Every section: core's own, then each feature's with its feature, in the registry's
    /// order.
    pub fn sections(&self) -> impl Iterator<Item = (Option<&'static Feature>, &'static Section)> {
        let core = CORE_SECTIONS.iter().map(|section| (None, *section));
        let features = self
            .features
            .iter()
            .flat_map(|feature| feature.settings.iter().map(move |s| (Some(*feature), s)));
        core.chain(features)
    }
}
/// The page `section` is on, with its label, group and place: `feature` is the section's,
/// none for core's.
pub fn page(feature: Option<&Feature>, section: &Section) -> (String, &'static str, Group, u16) {
    match (section.page, feature) {
        (Page::Shared(id), _) => {
            let page = shared_page(id).expect("a shared page, as the registry makes sure");
            (page.id.to_owned(), page.label, page.group, page.order)
        }
        (Page::Own, Some(feature)) => (
            own_page(feature),
            feature.switch.label,
            Group::Features,
            feature.place,
        ),
        (Page::Own, None) => unreachable!("core's sections are on shared pages"),
    }
}
/// A feature's own page: its switch's id, with `-` for `_`.
fn own_page(feature: &Feature) -> String {
    feature.switch.id.replace('_', "-")
}
fn shared_page(id: &str) -> Option<&'static SharedPage> {
    SHARED_PAGES.iter().find(|page| page.id == id)
}

/// Panics unless every section is as the slot asks, as the registry is installed: an id of
/// its own under its feature's name, snake_case after it, on a page that exists, under a
/// permission its owner declares and with a part its feature has; fields only a custom one
/// may lack, each with an id of its own and a default its kind holds.
pub(crate) fn check(registry: &Registry) {
    let mut ids = BTreeSet::new();
    for (feature, section) in registry.sections() {
        let id = section.id;
        let owner = feature.map_or("core", |feature| feature.name);
        assert!(ids.insert(id), "the section {id} is declared twice");
        assert!(
            !section.label.is_empty(),
            "{owner}'s section {id} has no label"
        );
        match feature {
            Some(feature) => check_feature(registry, feature, section),
            None => check_core(registry, section),
        }
        // Its record is kept as `section:<id>`, a member's own as `section:<id>:<user>`: an
        // id with a `:` in it would read as someone's own.
        assert!(
            id.split_once('.').is_some_and(|(_, what)| snake_case(what)),
            "{owner}'s section {id} isn't named <owner>.<what> with its what snake_case"
        );
        if let Page::Shared(page) = section.page {
            assert!(
                shared_page(page).is_some(),
                "{owner}'s section {id} is on {page}, which is no shared page"
            );
        }
        assert!(
            section.custom || !section.fields.is_empty(),
            "{owner}'s section {id} has no fields, so it must be custom"
        );
        assert!(
            section.kept.is_none() || section.scope == Scope::Dsp,
            "{owner}'s section {id} is each member's own, so nothing else keeps it"
        );
        let mut fields = BTreeSet::new();
        for field in section.fields {
            assert!(
                snake_case(field.id),
                "{id}'s field {} isn't snake_case",
                field.id
            );
            assert!(fields.insert(field.id), "{id} has two fields {}", field.id);
            assert!(
                !field.label.is_empty(),
                "{id}'s field {} has no label",
                field.id
            );
            if let Err(why) = check_field(field) {
                panic!("{id}'s field {}: {why}", field.id);
            }
        }
    }
}
/// Whether `name` is snake_case: lowercase letters, digits and `_`, from a letter.
fn snake_case(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
/// A feature's section: named under it, under its own permission, with one of its parts, on
/// a page of its own whose id no shared page takes; with a collection, one its feature keeps,
/// under the permission that sets when it runs.
fn check_feature(registry: &Registry, feature: &Feature, section: &Section) {
    let id = section.id;
    assert!(
        id.strip_prefix(feature.name)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|what| !what.is_empty()),
        "{}'s section {id} isn't named <feature>.<what>",
        feature.name
    );
    let parts = feature.subfeatures.iter();
    let own = feature
        .permissions
        .iter()
        .chain(parts.clone().flat_map(|sub| sub.permissions));
    assert!(
        section.permission == roles::ACCESS
            || own
                .clone()
                .any(|permission| permission.id == section.permission),
        "{}'s section {id} is under {}, which it doesn't declare",
        feature.name,
        section.permission
    );
    if let Some(part) = section.part {
        assert!(
            parts.clone().any(|sub| sub.id == part),
            "{}'s section {id} goes with {part}, which is none of its parts",
            feature.name
        );
    }
    if section.page == Page::Own {
        let page = own_page(feature);
        assert!(
            shared_page(&page).is_none(),
            "{}'s own page would be {page}, a shared page's id",
            feature.name
        );
    }
    if let Some(collection) = section.collection {
        let keeper = feature.keeps.iter().find(|keeper| {
            registry
                .collectors
                .iter()
                .flat_map(|collector| collector.collections())
                .any(|kept| kept.job_kind == keeper.keeps() && kept.schedule == collection)
        });
        assert!(
            keeper.is_some_and(|keeper| keeper.schedules().permission == section.permission),
            "{}'s section {id} goes with {collection}, which isn't a collection it keeps under {}",
            feature.name,
            section.permission
        );
        assert!(
            section.page == Page::Shared("collectors") && !section.custom,
            "{}'s section {id} goes with {collection}, so Settings draws it on Collectors",
            feature.name
        );
    }
}
/// One of core's: named under a core part, never a feature's name, under core's own
/// permission, on a shared page.
fn check_core(registry: &Registry, section: &Section) {
    let id = section.id;
    assert!(
        id.split_once('.').is_some_and(|(prefix, what)| {
            !prefix.is_empty()
                && !what.is_empty()
                && registry
                    .features
                    .iter()
                    .all(|feature| feature.name != prefix)
        }),
        "core's section {id} isn't named <core part>.<what>"
    );
    assert!(
        section.permission == roles::ACCESS
            || crate::manifest::permissions::CORE_PERMISSIONS
                .iter()
                .any(|permission| permission.id == section.permission),
        "core's section {id} is under {}, which core doesn't declare",
        section.permission
    );
    assert!(
        section.page != Page::Own && section.part.is_none() && section.collection.is_none(),
        "core's section {id} is on no feature's page and goes with no feature's part or collection"
    );
}
/// Why `field` doesn't hold, if it doesn't: its options are some, each once; its range is
/// one, its presets in it; and its default is a value it holds.
fn check_field(field: &Field) -> std::result::Result<(), String> {
    let distinct = |options: &[(&str, &str)]| {
        let values: BTreeSet<_> = options.iter().map(|(value, _)| value).collect();
        !options.is_empty() && values.len() == options.len()
    };
    match field.kind {
        Kind::Choice { options, .. }
        | Kind::Many {
            options: Options::Fixed(options),
            ..
        } if !distinct(options) => return Err("its options are none, or repeat a value".into()),
        Kind::Number {
            min, max, presets, ..
        } => {
            let values: BTreeSet<_> = presets.iter().map(|(value, _)| value).collect();
            if min > max
                || values.len() != presets.len()
                || presets
                    .iter()
                    .any(|(value, _)| !(min..=max).contains(value))
            {
                return Err("its range is none, or its presets repeat or leave it".into());
            }
        }
        Kind::Text { min, max, .. } if min > max => {
            return Err(format!("its least, {min}, is more than its most, {max}"));
        }
        _ => {}
    }
    let default = crate::settings::kinds::default(field);
    if !crate::settings::kinds::fits(field, &default) {
        return Err(format!("its default {default} isn't a value it holds"));
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/backend/settings.rs"]
mod tests;
