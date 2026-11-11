//! Metadata that reaches only the DSP owning a custom declaration.
use crate::{
    Result, State,
    accounts::{
        Context,
        api::types::{CustomFeature, CustomFeatures, CustomFrontend, CustomKind, CustomPermission},
    },
    db::Store,
    manifest::registry,
    tenancy::{catalog, roles},
};

pub(crate) fn view(db: &Store, state: &State, context: &Context) -> Result<Option<CustomFeatures>> {
    let code = db.find_dsp(&context.dsp.id)?.code;
    let owners: Vec<_> = registry()
        .features
        .iter()
        .filter(|feature| feature.owner.is_some() && feature.belongs_to(code.as_deref()))
        .collect();
    if owners.is_empty() {
        return Ok(None);
    }
    let catalog = catalog(code.as_deref());
    let mut declared: Vec<_> = owners
        .iter()
        .flat_map(|feature| {
            feature
                .permissions
                .iter()
                .chain(feature.subfeatures.iter().flat_map(|sub| sub.permissions))
        })
        .collect();
    declared.sort_by_key(|permission| (permission.order, permission.id));
    let permissions = declared
        .into_iter()
        .map(|permission| CustomPermission {
            id: permission.id.to_owned(),
            label: permission.label.to_owned(),
            group: permission.group.map(str::to_owned),
            implied: roles::granting(&[permission.id.to_owned()])
                .into_iter()
                .filter(|id| id != permission.id && roles::belongs_to(id, code.as_deref()))
                .collect(),
            under: permission
                .under
                .filter(|id| roles::belongs_to(id, code.as_deref()))
                .map(str::to_owned),
        })
        .collect();
    let frontends = owners
        .iter()
        .filter(|feature| context.has(feature.switch.id))
        .filter_map(|feature| {
            crate::server::http::custom_frontend(state, feature.name).map(|(url, styles)| {
                CustomFrontend {
                    name: feature.name.to_owned(),
                    url,
                    styles,
                    place: feature.place,
                }
            })
        })
        .collect();
    Ok(Some(CustomFeatures {
        catalog,
        permissions,
        frontends,
    }))
}

/// The custom switches and parts belonging to one DSP code, including those switched off.
pub(crate) fn catalog(code: Option<&str>) -> Vec<CustomFeature> {
    catalog::catalog()
        .iter()
        .filter(|entry| entry.owner.is_some() && entry.owner == code)
        .map(|entry| CustomFeature {
            id: entry.id.to_owned(),
            label: entry.label.to_owned(),
            place: registry()
                .features
                .iter()
                .find(|feature| {
                    feature.switch.id == entry.id
                        || matches!(entry.kind, catalog::Kind::Sub(page) if page == feature.switch.id)
                })
                .map_or(u16::MAX, |feature| feature.place),
            kind: match entry.kind {
                catalog::Kind::Sub(_) => CustomKind::Sub,
                _ => CustomKind::Page,
            },
            permissions: entry.permissions.iter().map(|id| (*id).to_owned()).collect(),
            requires: entry.requires.iter().map(|id| (*id).to_owned()).collect(),
            page: match entry.kind {
                catalog::Kind::Sub(page) => Some(page.to_owned()),
                _ => None,
            },
            tab: entry.tab,
            connects: entry.connects,
        })
        .collect()
}
