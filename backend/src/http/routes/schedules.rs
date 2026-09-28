//! Collection schedules. Every change here wakes the scheduler; a preview changes nothing.
//! The timecard page manages its collections' schedules and the routes page its own;
//! either permission opens the list, and a change needs the one its collection belongs to.
use crate::{
    Result,
    db::Store,
    http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read, write},
    },
    schedules::schedule_changes,
    validate as v,
};

const MANAGE: Dsp = Dsp("timecard.manage|routes.manage|dvic.manage");

/// The permission a schedule of `collection` needs, checked against the member's role
/// once more, as their features stand.
fn permitted(db: &Store, c: &Member, collection: &str) -> Result<()> {
    let permission = format!("{}.manage", crate::features::automation(collection));
    db.revalidate(c, &permission).map(|_| ())
}

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/dvic/schedules", Dsp("dvic.manage"), schedules),
        write("/api/dsp/dvic/schedules", Dsp("dvic.manage"), create).invalidates_schedules(),
        write(
            "/api/dsp/dvic/schedules/preview",
            Dsp("dvic.manage"),
            preview,
        ),
        write("/api/dsp/dvic/schedules/{key}", Dsp("dvic.manage"), update).invalidates_schedules(),
        write(
            "/api/dsp/dvic/schedules/{key}/enabled",
            Dsp("dvic.manage"),
            toggle,
        )
        .invalidates_schedules(),
        write(
            "/api/dsp/dvic/schedules/{key}/remove",
            Dsp("dvic.manage"),
            remove,
        )
        .invalidates_schedules(),
        read("/api/dsp/schedules", MANAGE, schedules),
        write("/api/dsp/schedules", MANAGE, create).invalidates_schedules(),
        write("/api/dsp/schedules/preview", MANAGE, preview),
        write("/api/dsp/schedules/{key}", MANAGE, update).invalidates_schedules(),
        write("/api/dsp/schedules/{key}/enabled", MANAGE, toggle).invalidates_schedules(),
        write("/api/dsp/schedules/{key}/remove", MANAGE, remove).invalidates_schedules(),
    ]
}

fn schedules(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let mut result = db.collection_schedules(c.dsp_id())?;
    let dvic = input.path.starts_with("/api/dsp/dvic/");
    result.schedules.retain(|s| {
        (s.collection.as_str() == "dvic") == dvic
            && c.can(&format!(
                "{}.manage",
                crate::features::automation(s.collection.as_str())
            ))
    });
    Reply::of(&result)
}

fn preview(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    Reply::of(&db.preview_schedule(c.dsp_id(), &input.body)?)
}

fn create(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let id = c.dsp_id();
    scope(
        input,
        v::text(&input.body, "collection", 1, 32).unwrap_or(""),
    )?;
    permitted(
        db,
        c,
        v::text(&input.body, "collection", 1, 32).unwrap_or(""),
    )?;
    let result = db.save_schedule(id, None, &input.body)?;
    let name = result.name.as_str();
    let subject = Some(("schedule", result.id.as_str()));
    let actor = Some(c.actor());
    db.audit_ref(
        actor,
        Some(id),
        "schedule.created",
        name,
        Some(name),
        &[],
        subject,
    )?;
    Reply::of_status(&result, 201)
}

fn update(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let (id, key) = (c.dsp_id(), input.param("key"));
    let before = db.collection_schedule(id, key)?;
    scope(input, before.collection.as_str())?;
    permitted(db, c, before.collection.as_str())?;
    let target = v::text(&input.body, "collection", 1, 32)?;
    scope(input, target)?;
    permitted(db, c, target)?;
    let result = db.save_schedule(id, Some(key), &input.body)?;
    db.audit_ref(
        Some(c.actor()),
        Some(id),
        "schedule.updated",
        &result.name,
        Some(&before.name),
        &schedule_changes(&before, &result),
        Some(("schedule", key)),
    )?;
    Reply::of(&result)
}

fn toggle(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let (id, key) = (c.dsp_id(), input.param("key"));
    let before = db.collection_schedule(id, key)?;
    scope(input, before.collection.as_str())?;
    permitted(db, c, before.collection.as_str())?;
    let result = db.enable_schedule(id, key, &input.body)?;
    db.audit_ref(
        Some(c.actor()),
        Some(id),
        "schedule.toggled",
        &result.name,
        Some(&result.name),
        &schedule_changes(&before, &result),
        Some(("schedule", key)),
    )?;
    Reply::of(&result)
}

fn remove(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let (id, key) = (c.dsp_id(), input.param("key"));
    let before = db.collection_schedule(id, key)?;
    scope(input, before.collection.as_str())?;
    permitted(db, c, before.collection.as_str())?;
    db.delete_collection_schedule(id, key, &input.body)?;
    let name = before.name.as_str();
    let subject = Some(("schedule", key));
    let actor = Some(c.actor());
    db.audit_ref(
        actor,
        Some(id),
        "schedule.deleted",
        name,
        Some(name),
        &[],
        subject,
    )?;
    Ok(Reply::ok())
}

fn scope(input: &Input, collection: &str) -> Result<()> {
    crate::ensure(
        !input.path.starts_with("/api/dsp/dvic/") || collection == "dvic",
        "permission_denied",
        403,
    )
}
