//! The DSP's categories of writeup: added, changed, retired and restored by those who manage
//! Conduct. Each change goes in the DSP's activity log, as a setting's does.
use crate::api::types::{CategoryInput, ConductCategory, ConductStepDown};
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::{Db, FromRow, Row, Store},
    ensure,
    foundation::crypto,
};
use rusqlite::params;

/// The most categories a DSP keeps in use at once.
const MOST: i64 = 100;

/// Every category, retired ones too, in the DSP's order, each with its step downs.
pub(crate) fn all(db: &Db) -> Result<Vec<ConductCategory>> {
    let mut categories: Vec<ConductCategory> = db.query_as(
        "SELECT * FROM conduct_categories ORDER BY retired, position, id",
        [],
    )?;
    let steps: Vec<CategoryStep> = db.query_as(
        "SELECT * FROM conduct_step_downs \
         ORDER BY CASE tier WHEN 'final' THEN 0 WHEN 'written' THEN 1 ELSE 2 END",
        [],
    )?;
    for CategoryStep { category, step } in steps {
        if let Some(kept) = categories.iter_mut().find(|c| c.id == category) {
            kept.step_downs.push(step);
        }
    }
    Ok(categories)
}

/// A step down as kept, with the category it's for.
struct CategoryStep {
    category: String,
    step: ConductStepDown,
}
impl FromRow for CategoryStep {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            category: row.get("category_id")?,
            step: ConductStepDown::from_row(row)?,
        })
    }
}
/// The category `id`, retired or not, without its step downs.
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
        keep_steps(&db, &id, &input.step_downs)?;
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
        keep_steps(&db, id, &input.step_downs)?;
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

/// Sets the category `id`'s step downs to `steps`, in place of those it had.
fn keep_steps(db: &Db, id: &str, steps: &[ConductStepDown]) -> Result<()> {
    db.exec("DELETE FROM conduct_step_downs WHERE category_id=?", [id])?;
    for step in steps {
        db.exec(
            "INSERT INTO conduct_step_downs (category_id,tier,after,unit,to_tier) VALUES (?,?,?,?,?)",
            params![id, step.tier, step.after, step.unit, step.to],
        )?;
    }
    Ok(())
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
