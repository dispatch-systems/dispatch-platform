//! Settings' Collectors page: each collection whose schedule the member sets, under the
//! permission its keeper names, with its rules saved together, switched on or off as a whole,
//! previewed unsaved, and collected now under the permission it runs under. Every change wakes
//! the scheduler; a preview changes nothing.
use crate::{
    Result,
    collection::schedules::Scheduled,
    db::Store,
    ensure,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read, write},
    },
};
use std::sync::LazyLock;

/// Every permission a collection's schedule is set under, any of which opens the page.
static SCHEDULING: LazyLock<String> =
    LazyLock::new(|| permissions(|scheduled| scheduled.keeper.schedules().permission));
/// Every permission a collection runs under.
static COLLECTING: LazyLock<String> =
    LazyLock::new(|| permissions(|scheduled| scheduled.keeper.permission()));
fn permissions(of: fn(Scheduled) -> &'static str) -> String {
    let mut permissions: Vec<&str> = Scheduled::all().map(of).collect();
    permissions.sort();
    permissions.dedup();
    permissions.join("|")
}

/// None in a build whose features keep no collection.
pub fn routes() -> Vec<Route> {
    if SCHEDULING.is_empty() {
        return vec![];
    }
    let scheduling = Dsp(SCHEDULING.as_str());
    vec![
        read("/api/dsp/collectors", scheduling, list),
        write(
            "/api/dsp/collectors/{collection}/schedule",
            scheduling,
            save,
        )
        .invalidates_schedules(),
        write(
            "/api/dsp/collectors/{collection}/enabled",
            scheduling,
            switch,
        )
        .invalidates_schedules(),
        write(
            "/api/dsp/collectors/{collection}/preview",
            scheduling,
            preview,
        ),
        write(
            "/api/dsp/collectors/{collection}/collect",
            Dsp(COLLECTING.as_str()),
            collect,
        )
        .bookkeeping(),
    ]
}

/// The collection the path names, while the member holds `permission` of it, as their role
/// and features stand.
fn named(
    db: &Store,
    c: &Member,
    input: &Input,
    permission: fn(Scheduled) -> &'static str,
) -> Result<Scheduled> {
    let scheduled = Scheduled::named(input.param("collection"))?;
    ensure(c.can(permission(scheduled)), "permission_denied", 403)?;
    db.revalidate(c, permission(scheduled))?;
    Ok(scheduled)
}
fn scheduling(scheduled: Scheduled) -> &'static str {
    scheduled.keeper.schedules().permission
}

fn list(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.collectors(c)?)
}

fn save(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let scheduled = named(db, c, input, scheduling)?;
    let id = c.dsp_id();
    if let Some(change) = db.save_rules(id, scheduled, &input.body)? {
        let label = scheduled.collection.label;
        db.audit_ref(
            Some(c.actor()),
            Some(id),
            "schedule.updated",
            label,
            Some(label),
            &[change],
            Some(("schedule", scheduled.collection.schedule)),
        )?;
    }
    let collects = c.can(scheduled.keeper.permission());
    Reply::of(&db.scheduled_collection(c.dsp_id(), scheduled, collects)?)
}

fn switch(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let scheduled = named(db, c, input, scheduling)?;
    let id = c.dsp_id();
    if db.switch_rules(id, scheduled, &input.body)? {
        let enabled = input.body["enabled"].as_bool() == Some(true);
        let label = scheduled.collection.label;
        let change = (
            "enabled",
            Some((!enabled).to_string()),
            Some(enabled.to_string()),
        );
        db.audit_ref(
            Some(c.actor()),
            Some(id),
            "schedule.toggled",
            label,
            Some(label),
            &[change],
            Some(("schedule", scheduled.collection.schedule)),
        )?;
    }
    let collects = c.can(scheduled.keeper.permission());
    Reply::of(&db.scheduled_collection(c.dsp_id(), scheduled, collects)?)
}

fn preview(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let scheduled = named(db, c, input, scheduling)?;
    Reply::of(&db.preview_rules(c.dsp_id(), scheduled, &input.body)?)
}

fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let scheduled = named(db, c, input, |scheduled| scheduled.keeper.permission())?;
    let id = c.dsp_id();
    let jobs = db.collect_now(id, c.actor(), scheduled, &input.body)?;
    db.audit(
        Some(c.actor()),
        Some(id),
        "collection.started",
        scheduled.collection.label,
    )?;
    Reply::of_status(&jobs, 202)
}
