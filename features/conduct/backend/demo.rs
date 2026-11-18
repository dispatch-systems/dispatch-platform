//! What a development or preview DSP shows of Conduct: three of its drivers written up, in four
//! categories and at every tier, some marked signed and some waiting on a signature.
use super::categories;
use dispatch_core::{
    Error, Result,
    db::{Store, iso},
    foundation::{clock, crypto},
    manifest::people::{self, PersonRole},
};
use jiff::{ToSpan, civil::Date};
use rusqlite::params;

/// A writeup the demo keeps: for which of the drivers, in which category, at which tier, with
/// the suspension a final one gives, how many days ago, whether it's signed, and what fills the
/// blanks of its category's wording, the writeup's own date filling `[date]`.
struct Demo {
    driver: usize,
    category: &'static str,
    tier: &'static str,
    suspension: Option<&'static str>,
    ago: i64,
    signed: bool,
    blanks: &'static [(&'static str, &'static str)],
}
const WRITEUPS: [Demo; 5] = [
    Demo {
        driver: 0,
        category: "safety",
        tier: "verbal",
        suspension: None,
        ago: 75,
        signed: true,
        blanks: &[
            ("[number]", "2"),
            ("[events]", "following distance and a rolling stop"),
        ],
    },
    Demo {
        driver: 0,
        category: "safety",
        tier: "written",
        suspension: None,
        ago: 12,
        signed: false,
        blanks: &[
            ("[number]", "3"),
            ("[events]", "distracted driving and two rolling stops"),
        ],
    },
    Demo {
        driver: 1,
        category: "tardies",
        tier: "verbal",
        suspension: None,
        ago: 40,
        signed: true,
        blanks: &[("[dates]", "three days of the two weeks before")],
    },
    Demo {
        driver: 1,
        category: "uniform",
        tier: "verbal",
        suspension: None,
        ago: 2,
        signed: false,
        blanks: &[("[missing item]", "a safety vest")],
    },
    Demo {
        driver: 2,
        category: "damage",
        tier: "final",
        suspension: Some("3 days"),
        ago: 20,
        signed: true,
        blanks: &[
            ("[van]", "Van 14"),
            ("[damage]", "the passenger side mirror"),
        ],
    },
];

/// Writes the DSP's drivers up as `WRITEUPS` says, the first three as the directory lists them,
/// by the platform's owner, dated back from today where the DSP is.
pub fn demo(store: &Store, dsp: &str, timezone: &str) -> Result<()> {
    let everyone = people::read(store, dsp)?;
    let drivers: Vec<_> = everyone
        .iter()
        .filter(|person| person.role == PersonRole::Driver)
        .take(3)
        .collect();
    let supervisor = store
        .active_platform_owners()?
        .into_iter()
        .next()
        .ok_or_else(|| Error::new("platform_owner_required", 409))?;
    let (supervisor_id, supervisor) = (supervisor.id.clone(), supervisor.name());
    let today = clock::today(&clock::zone(timezone)?);
    let db = store.dsp(dsp)?;
    db.transaction(|| {
        for writeup in WRITEUPS.iter().filter(|w| w.driver < drivers.len()) {
            let person = drivers[writeup.driver];
            let category = categories::one(&db, writeup.category)?;
            let written: Date = today.saturating_sub(writeup.ago.days());
            let printed = written.strftime("%m/%d/%Y").to_string();
            let statement = writeup
                .blanks
                .iter()
                .fold(category.statement, |text, (blank, with)| {
                    text.replace(blank, with)
                })
                .replace("[date]", &printed);
            // Signed the day after it was written.
            let signed = writeup
                .signed
                .then(|| written.saturating_add(1.day()).to_string());
            let at = iso();
            db.exec(
                "INSERT INTO conduct_writeups (id,person,person_name,category_id,tier,final_kind,\
                 suspension,written_on,supervisor_id,supervisor_name,statement,prior,plan,\
                 created_at,updated_at,signed_on,signed_by_name,signed_at) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                params![
                    crypto::id("writeup")?,
                    person.code,
                    person.name,
                    category.id,
                    writeup.tier,
                    writeup.suspension.map(|_| "suspension"),
                    writeup.suspension,
                    written.to_string(),
                    supervisor_id,
                    supervisor,
                    statement,
                    category.prior,
                    category.plan,
                    at,
                    at,
                    signed,
                    signed.as_ref().map(|_| &supervisor),
                    signed.as_ref().map(|_| &at),
                ],
            )?;
        }
        Ok(())
    })
}
