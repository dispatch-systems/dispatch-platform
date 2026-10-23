//! A DSP's Settings, as its members see and save it: every section they may see, and saving
//! one. The DSP is always the one the member's view authorized, never one a request names.
use super::types::{
    SettingsAnswer, SettingsField, SettingsGroup, SettingsPage, SettingsSave, SettingsSaved,
    SettingsScope, SettingsSection,
};
use crate::{
    Error, Result,
    accounts::Context,
    db::Store,
    ensure,
    foundation::{crypto, observability},
    manifest::{
        Feature, registry,
        settings::{self as declared, Group, Scope, Section},
    },
    server::{
        cache::Scope as Cache,
        http::{
            input::{Input, Reply},
            route::{Dsp, Member, Route, read, write},
        },
    },
    settings::{self, Saved, kinds, store},
    tenancy::roles,
};
use serde_json::json;

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/settings", Dsp(roles::ACCESS), answer),
        // A save that retimes the DSP's schedules, as its timezone does, wakes the scheduler
        // itself; any other leaves it be.
        write("/api/dsp/settings/{section}", Dsp(roles::ACCESS), save),
    ]
}

/// Served from the read cache, under the DSP's own name, for what the member's access makes
/// of it: one answer for every member with the same role, permissions and features, unless
/// it holds values of their own. Any write to the DSP's data but a collection's live pages
/// evicts it.
fn answer(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    let seen = registry()
        .sections()
        .filter(|(feature, section)| visible(c, *feature, section));
    let own = own(c, seen.map(|(_, section)| section));
    let access = serde_json::to_vec(&(own, &c.role, c.owner, &c.permissions, &c.features))?;
    let bytes = c.state.read_cache.json(
        Cache::tenant(settings::CACHED, c.dsp_id()),
        crypto::sha(access),
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || sections(db, c, registry().sections()),
    )?;
    Ok(Reply::encoded(bytes))
}
fn save(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let id = input.param("section");
    Reply::of(&save_as(db, c, registry().sections(), id, &input.body)?)
}

/// A section of `declared`, with its feature, none for core's.
type Declared = (Option<&'static Feature>, &'static Section);
/// The page a section is on: its id, label, group and place.
type OnPage = (String, &'static str, Group, u16);

/// Every section of `declared` the member may see, with the pages they're on, read from the
/// DSP's stored settings at once, and from the member's own only when the answer holds them
/// (`own`). One that can't be read now, as when its options can't be listed, is left out, and
/// noted in the server's log; the rest are as they are.
pub(crate) fn sections(
    db: &Store,
    c: &Context,
    declared: impl IntoIterator<Item = Declared>,
) -> Result<SettingsAnswer> {
    let mut seen: Vec<(Declared, OnPage)> = declared
        .into_iter()
        .filter(|(feature, section)| visible(c, *feature, section))
        .map(|(feature, section)| ((feature, section), declared::page(feature, section)))
        .collect();
    seen.sort_by(
        |((_, a), (page_a, _, group_a, at_a)), ((_, b), (page_b, _, group_b, at_b))| {
            (group_a, at_a, page_a, a.order, a.id).cmp(&(group_b, at_b, page_b, b.order, b.id))
        },
    );
    let own = own(c, seen.iter().map(|((_, section), _)| *section));
    let records = store::Records::load(db, &c.dsp.id, own)?;
    let mut lists = kinds::Lists::new(db, &c.dsp.id);
    let mut pages: Vec<SettingsPage> = Vec::new();
    let mut sections = Vec::with_capacity(seen.len());
    for ((feature, section), (page, label, group, order)) in seen {
        let member = (section.scope == Scope::Member).then(|| c.actor());
        let record = records.of(section, member);
        let shown = match view(db, c, &mut lists, feature, section, page.clone(), record) {
            Ok(shown) => shown,
            Err(error) => {
                skipped(c, section, &error);
                continue;
            }
        };
        if pages.last().is_none_or(|last| last.id != page) {
            pages.push(SettingsPage {
                id: page,
                label: label.to_owned(),
                group: match group {
                    Group::Personal => SettingsGroup::Personal,
                    Group::Dsp => SettingsGroup::Dsp,
                    Group::Features => SettingsGroup::Features,
                },
                order,
            });
        }
        sections.push(shown);
    }
    Ok(SettingsAnswer { pages, sections })
}
/// Whose own values a member's Settings holds, given the sections they see: theirs while one
/// of them has fields each member keeps their own of, and nobody's otherwise, so that one
/// answer, cached once, serves every member with the same access. A section of each
/// member's own with no fields, as their profile's, holds nothing of theirs to answer.
fn own(c: &Context, seen: impl IntoIterator<Item = &'static Section>) -> Option<&str> {
    let theirs = |section: &Section| section.scope == Scope::Member && !section.fields.is_empty();
    seen.into_iter().any(theirs).then(|| c.actor())
}
/// Notes in the server's log a section left out of a member's Settings: which, of which DSP,
/// and why, never its values.
fn skipped(c: &Context, section: &Section, error: &Error) {
    observability::event(
        "error",
        "settings.section_skipped",
        json!({"dspId": c.dsp.id, "section": section.id, "error": error.code}),
    );
}

/// Saves the section `id` of `declared` as `body` asks, once the member may see it and holds
/// its permission: the section as it is now, its options as the save listed them, or the
/// words to confirm first.
pub(crate) fn save_as(
    db: &Store,
    c: &Context,
    declared: impl IntoIterator<Item = Declared>,
    id: &str,
    body: &serde_json::Value,
) -> Result<SettingsSaved> {
    let (feature, section) = declared
        .into_iter()
        .find(|(feature, section)| section.id == id && shown(c, *feature, section))
        .ok_or_else(|| Error::new("not_found", 404))?;
    ensure(c.allows(section.permission), "permission_denied", 403)?;
    let body: SettingsSave =
        serde_json::from_value(body.clone()).map_err(|_| Error::new("invalid_input", 400))?;
    let (saved, mut lists) = settings::save_listing(
        db,
        &c.dsp.id,
        c.actor(),
        section,
        &serde_json::Value::Object(body.values),
        body.revision,
        body.confirmed.unwrap_or(false),
    )?;
    Ok(match saved {
        Saved::Saved { .. } => {
            let (page, ..) = declared::page(feature, section);
            let member = (section.scope == Scope::Member).then(|| c.actor());
            let record = store::record(db, &c.dsp.id, section, member)?;
            SettingsSaved::Section {
                section: Box::new(view(db, c, &mut lists, feature, section, page, &record)?),
                reopen: db.find_dsp(&c.dsp.id)?.revision != c.dsp.revision,
            }
        }
        Saved::Confirm(warning) => SettingsSaved::Warning { warning },
    })
}

/// Whether the member sees `section`: its feature, and its part, on for the DSP and shown to
/// them, and its permission held.
fn visible(c: &Context, feature: Option<&Feature>, section: &Section) -> bool {
    shown(c, feature, section) && c.allows(section.permission)
}
/// Whether the DSP has `section` as the member sees it: core's always, a feature's while it,
/// and its part, are on and not hidden from them.
fn shown(c: &Context, feature: Option<&Feature>, section: &Section) -> bool {
    feature.is_none_or(|feature| c.has(feature.switch.id))
        && section.part.is_none_or(|part| c.has(part))
}

/// `section` as the member sees it, on `page`, by its `record`, its fields' options from
/// `lists`.
fn view(
    db: &Store,
    c: &Context,
    lists: &mut kinds::Lists,
    feature: Option<&Feature>,
    section: &Section,
    page: String,
    record: &serde_json::Value,
) -> Result<SettingsSection> {
    let now = store::current(db, &c.dsp.id, section, record)?;
    let fields = section
        .fields
        .iter()
        .map(|field| {
            Ok(SettingsField {
                id: field.id.to_owned(),
                label: field.label.to_owned(),
                kind: kinds::described(field, lists.options(field)?)?,
            })
        })
        .collect::<Result<_>>()?;
    Ok(SettingsSection {
        id: section.id.to_owned(),
        page,
        label: section.label.to_owned(),
        order: section.order,
        scope: match section.scope {
            Scope::Dsp => SettingsScope::Dsp,
            Scope::Member => SettingsScope::Member,
        },
        custom: section.custom,
        collection: section.collection.map(str::to_owned),
        fields,
        values: now.values,
        revision: now.revision,
        sources: feature.map(sources).unwrap_or_default(),
        keywords: section
            .keywords
            .iter()
            .map(|&word| word.to_owned())
            .chain(section.found.map(|found| found(c)).unwrap_or_default())
            .collect(),
    })
}
/// The labels of the collectors whose collections `feature` keeps, in the registry's order.
fn sources(feature: &Feature) -> Vec<String> {
    registry()
        .collectors
        .iter()
        .filter(|collector| {
            collector.collections().iter().any(|collection| {
                feature
                    .keeps
                    .iter()
                    .any(|keeper| keeper.keeps() == collection.job_kind)
            })
        })
        .map(|collector| collector.label().to_owned())
        .collect()
}

#[cfg(test)]
#[path = "../tests/backend/routes.rs"]
mod tests;
