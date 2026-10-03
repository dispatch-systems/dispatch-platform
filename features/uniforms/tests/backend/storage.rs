//! Uniform Inventory's storage, against a real store in a temporary directory: the catalog,
//! the stock counts and the journal both write to.
use super::*;
use crate::{
    accounts::Auth,
    config::Config,
    contracts::{UniformEventKind, UniformFit, UniformVariantInput},
    db::s,
    operations,
};
use std::os::unix::fs::PermissionsExt;

/// A preview platform's first DSP, every feature on, and its owner managing its uniforms.
fn ready() -> (tempfile::TempDir, Store, Context) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = Config::load().unwrap();
    config.root = root.path().into();
    config.development = true;
    config.fixture = true;
    config.environment = "preview".into();
    let store = Store::initialize(config).unwrap();
    let bootstrap = operations::bootstrap(
        &store,
        "uniforms@example.test",
        "Uniform",
        "Owner",
        "test-password-long",
    )
    .unwrap();
    let auth = Auth {
        user: serde_json::from_value(bootstrap["owner"].clone()).unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
    };
    let context = store
        .context(&auth, s(&bootstrap["dsp"], "id"), "uniforms.manage")
        .unwrap();
    (root, store, context)
}

fn polo(revision: Option<i64>, variants: &[(Option<&str>, UniformFit, &str)]) -> UniformInput {
    UniformInput {
        name: "Polo".into(),
        category: "Tops".into(),
        revision,
        variants: variants
            .iter()
            .map(|(id, fit, size)| UniformVariantInput {
                id: id.map(str::to_owned),
                fit: *fit,
                size: (*size).to_owned(),
            })
            .collect(),
    }
}

/// A request id of the length adjustments accept, distinct for each `n`.
fn request(n: u32) -> String {
    format!("adjustment-{n:08}")
}

#[test]
fn an_inventory_starts_empty_and_is_initialized_once() {
    let (_root, store, c) = ready();
    let dsp = c.dsp.id.clone();
    let empty = store.uniform_inventory(&dsp).unwrap();
    assert_eq!(empty.revision, 0);
    assert!(empty.uniforms.is_empty());

    let started = store.initialize_uniforms(&c, true).unwrap();
    assert_eq!(started.revision, 1);
    let names: Vec<&str> = started.uniforms.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(names.len(), 11);
    assert_eq!(names[0], "Short Sleeve Polo");
    assert!(
        started
            .uniforms
            .iter()
            .flat_map(|u| &u.variants)
            .all(|v| v.quantity == 0)
    );
    let again = store.initialize_uniforms(&c, true).unwrap_err();
    assert_eq!(again.code, "uniform_inventory_initialized");
    assert_eq!(store.uniform_inventory(&dsp).unwrap().revision, 1);
}

#[test]
fn stock_moves_one_at_a_time_never_below_zero_and_a_stocked_uniform_stays() {
    let (_root, store, c) = ready();
    let dsp = c.dsp.id.clone();
    store.initialize_uniforms(&c, false).unwrap();
    let created = store
        .save_uniform(
            &c,
            None,
            &polo(
                None,
                &[(None, UniformFit::Men, "M"), (None, UniformFit::Women, "S")],
            ),
        )
        .unwrap();
    let uniform = created.uniforms[0].clone();
    assert_eq!(uniform.variants.len(), 2);
    let size = uniform.variants[0].id.clone();

    let below = store
        .adjust_uniform(&c, &size, -1, &request(1))
        .unwrap_err();
    assert_eq!(below.code, "uniform_out_of_stock");
    let wide = store.adjust_uniform(&c, &size, 2, &request(2)).unwrap_err();
    assert_eq!(wide.code, "invalid_input");
    let added = store.adjust_uniform(&c, &size, 1, &request(3)).unwrap();
    assert_eq!(
        (added.variant_id.as_str(), added.quantity),
        (size.as_str(), 1)
    );
    // The same request again answers what it did, and counts once.
    let repeated = store.adjust_uniform(&c, &size, 1, &request(3)).unwrap();
    assert_eq!(
        (repeated.revision, repeated.quantity),
        (added.revision, added.quantity)
    );
    let stocked = store.uniform_inventory(&dsp).unwrap();
    assert_eq!(stocked.uniforms[0].variants[0].quantity, 1);

    let kept = store
        .archive_uniform(&c, &uniform.id, uniform.revision)
        .unwrap_err();
    assert_eq!(kept.code, "uniform_in_stock");
    store.adjust_uniform(&c, &size, -1, &request(4)).unwrap();
    let archived = store
        .archive_uniform(&c, &uniform.id, uniform.revision)
        .unwrap();
    assert!(archived.uniforms.is_empty());

    let kinds: Vec<UniformEventKind> = store
        .uniform_history(&dsp, i64::MAX)
        .unwrap()
        .events
        .iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            UniformEventKind::Archived,
            UniformEventKind::Adjusted,
            UniformEventKind::Adjusted,
            UniformEventKind::Created,
            UniformEventKind::Initialized,
        ]
    );
}

#[test]
fn updates_bring_adjustments_until_the_catalog_changes() {
    let (_root, store, c) = ready();
    let dsp = c.dsp.id.clone();
    store.initialize_uniforms(&c, false).unwrap();
    let before = store.uniform_inventory(&dsp).unwrap().revision;
    let created = store
        .save_uniform(&c, None, &polo(None, &[(None, UniformFit::Unisex, "L")]))
        .unwrap();
    let size = created.uniforms[0].variants[0].id.clone();
    store.adjust_uniform(&c, &size, 1, &request(1)).unwrap();

    let since_created = store.uniform_updates(&dsp, created.revision).unwrap();
    assert!(since_created.inventory.is_none());
    assert_eq!(since_created.adjustments.len(), 1);
    assert_eq!(since_created.adjustments[0].quantity, 1);
    // A catalog edit since then sends the whole inventory instead.
    let since_before = store.uniform_updates(&dsp, before).unwrap();
    assert!(since_before.adjustments.is_empty());
    let inventory = since_before.inventory.unwrap();
    assert_eq!(inventory.revision, since_before.revision);
    assert_eq!(inventory.uniforms[0].variants[0].quantity, 1);
    // Renaming a size the same way keeps its stock.
    let uniform = &inventory.uniforms[0];
    let renamed = store
        .save_uniform(
            &c,
            Some(&uniform.id),
            &polo(
                Some(uniform.revision),
                &[(Some(&size), UniformFit::Unisex, "Large")],
            ),
        )
        .unwrap();
    assert_eq!(renamed.uniforms[0].variants[0].size, "Large");
    assert_eq!(renamed.uniforms[0].variants[0].quantity, 1);
}
