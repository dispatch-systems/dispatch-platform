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
    foundation::crypto,
    manifest::{
        Feature, registry,
        settings::{self as declared, Group, Scope, Section},
    },
    server::{
        cache::Scope as Cache,
        http::{
            input::{Input, Reply},
            route::{Dsp, Member, Route, needs_recent_verification, read, write},
        },
    },
    settings::{self, Saved, kinds, store},
    tenancy::roles,
};

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/settings", Dsp(roles::ACCESS), answer),
        // A section's `saved` may move what the DSP's schedules wait on, as its timezone.
        write("/api/dsp/settings/{section}", Dsp(roles::ACCESS), save).invalidates_schedules(),
    ]
}

/// Served from the read cache, under the DSP's own name, for what the member's access makes
/// of it: any write to the DSP's data but a collection's live pages evicts it.
fn answer(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    let access = serde_json::to_vec(&(c.actor(), &c.role, c.owner, &c.permissions, &c.features))?;
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
/// DSP's stored settings at once.
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
    let mut pages: Vec<SettingsPage> = Vec::new();
    for (_, (id, label, group, order)) in &seen {
        if pages.last().is_none_or(|page| page.id != *id) {
            pages.push(SettingsPage {
                id: id.clone(),
                label: (*label).to_owned(),
                group: match group {
                    Group::Personal => SettingsGroup::Personal,
                    Group::Dsp => SettingsGroup::Dsp,
                    Group::Features => SettingsGroup::Features,
                },
                order: *order,
            });
        }
    }
    let records = store::Records::load(db, &c.dsp.id, c.actor())?;
    let sections = seen
        .into_iter()
        .map(|((feature, section), (page, ..))| {
            let member = (section.scope == Scope::Member).then(|| c.actor());
            view(db, c, feature, section, page, records.of(section, member))
        })
        .collect::<Result<_>>()?;
    Ok(SettingsAnswer { pages, sections })
}

/// Saves the section `id` of `declared` as `body` asks, once the member may see it and holds
/// its permission, recently verified when that asks for it: the section as it is now, or the
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
    if needs_recent_verification(section.permission) {
        db.ensure_recent(&c.auth)?;
    }
    let body: SettingsSave =
        serde_json::from_value(body.clone()).map_err(|_| Error::new("invalid_input", 400))?;
    let saved = settings::save(
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
                section: Box::new(view(db, c, feature, section, page, &record)?),
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

/// `section` as the member sees it, on `page`, by its `record`.
fn view(
    db: &Store,
    c: &Context,
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
                kind: kinds::described(field, kinds::options(db, &c.dsp.id, field)?)?,
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
