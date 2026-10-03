//! Collection schedules. Every change here wakes the scheduler; a preview changes nothing.
//! The generic routes are the timecard page's alone. Routes, DVIC and the scorecard have
//! routes of their own, behind their own permissions, and the generic routes never touch
//! their schedules.
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

const MANAGE: Dsp = Dsp("timecard.manage");
const ROUTES: Dsp = Dsp("routes.manage");
const SCORECARD: Dsp = Dsp("scorecard.manage");

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
        read("/api/dsp/scorecard/schedules", SCORECARD, schedules),
        write("/api/dsp/scorecard/schedules", SCORECARD, create).invalidates_schedules(),
        write("/api/dsp/scorecard/schedules/preview", SCORECARD, preview),
        write("/api/dsp/scorecard/schedules/{key}", SCORECARD, update).invalidates_schedules(),
        write(
            "/api/dsp/scorecard/schedules/{key}/enabled",
            SCORECARD,
            toggle,
        )
        .invalidates_schedules(),
        write(
            "/api/dsp/scorecard/schedules/{key}/remove",
            SCORECARD,
            remove,
        )
        .invalidates_schedules(),
        read("/api/dsp/routes/schedules", ROUTES, schedules),
        write("/api/dsp/routes/schedules", ROUTES, create).invalidates_schedules(),
        write("/api/dsp/routes/schedules/preview", ROUTES, preview),
        write("/api/dsp/routes/schedules/{key}", ROUTES, update).invalidates_schedules(),
        write("/api/dsp/routes/schedules/{key}/enabled", ROUTES, toggle).invalidates_schedules(),
        write("/api/dsp/routes/schedules/{key}/remove", ROUTES, remove).invalidates_schedules(),
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
    let home = home(input);
    result.schedules.retain(|s| {
        let page = crate::features::automation(s.collection.as_str());
        page == home && c.can(&format!("{page}.manage"))
    });
    Reply::of(&result)
}

fn preview(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    // A schedule named for its timing is read only through its own feature's routes.
    if input.body.get("scheduleId").is_some() {
        let key = v::text(&input.body, "scheduleId", 1, 128)?;
        scope(
            input,
            db.collection_schedule(c.dsp_id(), key)?.collection.as_str(),
        )?;
    }
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

/// The feature whose schedule routes a request came to: one with a collection of its own,
/// under `/api/dsp/<feature>/schedules`, or the timecard page's generic ones.
fn home(input: &Input) -> &'static str {
    let segment = input
        .path
        .strip_prefix("/api/dsp/")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    crate::features::automation(segment)
}
/// Each feature's routes change only the schedules of the collections it owns, as the
/// catalog says, so a feature added later is apart from the generic routes without a list
/// to keep.
fn scope(input: &Input, collection: &str) -> Result<()> {
    crate::ensure(
        crate::features::automation(collection) == home(input),
        "permission_denied",
        403,
    )
}
