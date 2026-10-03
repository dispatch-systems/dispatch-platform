//! Collection schedules. Each feature offers the schedules of the collections it keeps
//! under routes of its own, behind its own permission, built here; the timecard page's are
//! the generic ones, and never touch another feature's schedules. Every change here wakes
//! the scheduler; a preview changes nothing.
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
use std::{
    collections::BTreeSet,
    sync::{Mutex, PoisonError},
};

/// Whose schedules a set of routes changes: the page that runs their collections, and
/// the permission that manages them.
#[derive(Clone, Copy)]
struct Owner {
    page: &'static str,
    access: Dsp,
}

/// The six routes of the schedules of `page`'s collections, under `prefix`, for a member
/// with `access`: list, create, preview, update, switch on or off, and remove.
pub fn schedule_routes(prefix: &'static str, access: Dsp, page: &'static str) -> Vec<Route> {
    let owner = Owner { page, access };
    vec![
        read(prefix, access, move |db: &Store, c: &Member, _: &Input| {
            schedules(db, c, owner)
        }),
        write(
            prefix,
            access,
            move |db: &Store, c: &Member, input: &Input| create(db, c, input, owner),
        )
        .invalidates_schedules(),
        write(
            path(prefix, "/preview"),
            access,
            move |db: &Store, c: &Member, input: &Input| preview(db, c, input, owner),
        ),
        write(
            path(prefix, "/{key}"),
            access,
            move |db: &Store, c: &Member, input: &Input| update(db, c, input, owner),
        )
        .invalidates_schedules(),
        write(
            path(prefix, "/{key}/enabled"),
            access,
            move |db: &Store, c: &Member, input: &Input| toggle(db, c, input, owner),
        )
        .invalidates_schedules(),
        write(
            path(prefix, "/{key}/remove"),
            access,
            move |db: &Store, c: &Member, input: &Input| remove(db, c, input, owner),
        )
        .invalidates_schedules(),
    ]
}
/// `prefix` and `suffix` joined, as the `'static` path a route is registered under: made
/// once each, however often the route table is built.
fn path(prefix: &'static str, suffix: &str) -> &'static str {
    static PATHS: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let path = format!("{prefix}{suffix}");
    let mut paths = PATHS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(made) = paths.get(path.as_str()) {
        return made;
    }
    let made: &'static str = Box::leak(path.into_boxed_str());
    paths.insert(made);
    made
}

/// The member may still manage the owner's schedules, as their role and features stand.
fn permitted(db: &Store, c: &Member, owner: Owner) -> Result<()> {
    db.revalidate(c, owner.access.0).map(|_| ())
}

fn schedules(db: &Store, c: &Member, owner: Owner) -> Result<Reply> {
    let mut result = db.collection_schedules(c.dsp_id())?;
    result.schedules.retain(|s| {
        let page = crate::features::automation(s.collection.as_str());
        page == owner.page && c.can(owner.access.0)
    });
    Reply::of(&result)
}

fn preview(db: &Store, c: &Member, input: &Input, owner: Owner) -> Result<Reply> {
    // A schedule named for its timing is read only through its own feature's routes.
    if input.body.get("scheduleId").is_some() {
        let key = v::text(&input.body, "scheduleId", 1, 128)?;
        scope(
            owner,
            db.collection_schedule(c.dsp_id(), key)?.collection.as_str(),
        )?;
    }
    Reply::of(&db.preview_schedule(c.dsp_id(), &input.body)?)
}

fn create(db: &Store, c: &Member, input: &Input, owner: Owner) -> Result<Reply> {
    let id = c.dsp_id();
    scope(
        owner,
        v::text(&input.body, "collection", 1, 32).unwrap_or(""),
    )?;
    permitted(db, c, owner)?;
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

fn update(db: &Store, c: &Member, input: &Input, owner: Owner) -> Result<Reply> {
    let (id, key) = (c.dsp_id(), input.param("key"));
    let before = db.collection_schedule(id, key)?;
    scope(owner, before.collection.as_str())?;
    permitted(db, c, owner)?;
    let target = v::text(&input.body, "collection", 1, 32)?;
    scope(owner, target)?;
    permitted(db, c, owner)?;
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

fn toggle(db: &Store, c: &Member, input: &Input, owner: Owner) -> Result<Reply> {
    let (id, key) = (c.dsp_id(), input.param("key"));
    let before = db.collection_schedule(id, key)?;
    scope(owner, before.collection.as_str())?;
    permitted(db, c, owner)?;
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

fn remove(db: &Store, c: &Member, input: &Input, owner: Owner) -> Result<Reply> {
    let (id, key) = (c.dsp_id(), input.param("key"));
    let before = db.collection_schedule(id, key)?;
    scope(owner, before.collection.as_str())?;
    permitted(db, c, owner)?;
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

/// Each feature's routes change only the schedules of the collections it keeps, as the
/// catalog says, so a feature added later is apart from the others' without a list to
/// keep.
fn scope(owner: Owner, collection: &str) -> Result<()> {
    crate::ensure(
        crate::features::automation(collection) == owner.page,
        "permission_denied",
        403,
    )
}
