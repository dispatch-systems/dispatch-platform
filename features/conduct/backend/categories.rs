//! The DSP's categories of writeup: added, changed, retired and restored by those who manage
//! Conduct. Each change goes in the DSP's activity log, as a setting's does.
use crate::api::types::{CategoryInput, ConductCategory};
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::{Db, Store},
    ensure,
    foundation::crypto,
};
use rusqlite::params;

/// The most categories a DSP keeps in use at once.
const MOST: i64 = 100;

/// Every category, retired ones too, in the DSP's order.
pub(crate) fn all(db: &Db) -> Result<Vec<ConductCategory>> {
    db.query_as(
        "SELECT * FROM conduct_categories ORDER BY retired, position, id",
        [],
    )
}
/// The category `id`, retired or not.
pub(crate) fn one(db: &Db, id: &str) -> Result<ConductCategory> {
    db.one_as("SELECT * FROM conduct_categories WHERE id=?", [id])?
        .ok_or_else(|| Error::new("conduct_category_not_found", 404))
}

pub(crate) fn add(store: &Store, c: &Context, input: &CategoryInput) -> Result<()> {
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        ensure(
            db.count(
                "SELECT count(*) FROM conduct_categories WHERE retired=0",
                [],
            )? < MOST,
            "conduct_category_limit",
            409,
        )?;
        let id = crypto::id("category")?;
        name_free(&db, &input.name, &id)?;
        db.exec(
            "INSERT INTO conduct_categories (id,name,start_tier,statement,prior,plan,position) \
             VALUES (?,?,?,?,?,?,(SELECT coalesce(max(position),0)+1 FROM conduct_categories))",
            params![
                id,
                input.name,
                input.start_tier,
                input.statement,
                input.prior,
                input.plan
            ],
        )?;
        c.audit(store, "conduct.category_added", &input.name)
    })
}

pub(crate) fn change(store: &Store, c: &Context, id: &str, input: &CategoryInput) -> Result<()> {
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        let before = one(&db, id)?;
        ensure(!before.retired, "conduct_category_not_found", 404)?;
        ensure(
            input.revision == Some(before.revision),
            "conduct_category_changed",
            409,
        )?;
        name_free(&db, &input.name, id)?;
        db.exec(
            "UPDATE conduct_categories SET name=?,start_tier=?,statement=?,prior=?,plan=?,\
             revision=revision+1 WHERE id=?",
            params![
                input.name,
                input.start_tier,
                input.statement,
                input.prior,
                input.plan,
                id
            ],
        )?;
        c.audit(store, "conduct.category_changed", &input.name)
    })
}

/// Retires the category `id`, or with `restore` puts it back in use.
pub(crate) fn retire(store: &Store, c: &Context, id: &str, restore: bool) -> Result<()> {
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        let category = one(&db, id)?;
        ensure(category.retired == restore, "conduct_category_changed", 409)?;
        if restore {
            name_free(&db, &category.name, id)?;
        }
        db.exec(
            "UPDATE conduct_categories SET retired=?,revision=revision+1 WHERE id=?",
            params![!restore, id],
        )?;
        let action = if restore {
            "conduct.category_restored"
        } else {
            "conduct.category_retired"
        };
        c.audit(store, action, &category.name)
    })
}

/// Refuses `name` while another category in use has it.
fn name_free(db: &Db, name: &str, id: &str) -> Result<()> {
    ensure(
        db.count(
            "SELECT count(*) FROM conduct_categories WHERE name=? COLLATE NOCASE AND retired=0 AND id!=?",
            [name, id],
        )? == 0,
        "conduct_category_name_taken",
        409,
    )
}
