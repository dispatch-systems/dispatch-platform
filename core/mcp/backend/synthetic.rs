//! A synthetic DSP to try agents against: two weeks of Northline's demo people as every
//! source would hold them. Core decides each person's days; each feature writes down what
//! its sources would hold of them, in a step of its own, routes and meal breaks through the
//! same staging and publishing a collection uses, so they read exactly as collected data
//! does. Only a development server with fixture providers, seeded first, can make it.
use crate::{
    Error, Result,
    collection::registry::Provider,
    db::{Store, s},
    ensure,
    manifest::registry,
};
use chrono::{Duration, NaiveDate, TimeZone};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// How many days end with yesterday.
pub const DAYS: i64 = 14;
pub const STATION: &str = "DEMO1";

/// What a feature adds to the synthetic DSP, as its manifest's `mcp` declares it.
pub struct Synthetic {
    /// The DSP's people, from the feature whose roster names them.
    pub people: Option<Roster>,
    /// What it writes down, step by step.
    pub steps: &'static [Step],
}
impl Synthetic {
    pub const NONE: Self = Self {
        people: None,
        steps: &[],
    };
}
/// The synthetic DSP's people, given its timezone, its dispatcher first.
pub type Roster = fn(&str) -> Result<Vec<Value>>;
/// One step of filling the synthetic DSP, with its place in the order every feature's
/// steps run in.
pub struct Step {
    pub order: u16,
    pub run: fn(&Store, &mut World) -> Result<Made>,
}
/// What a step made, under a name, for what `seed-agents` prints; nothing to say for some.
pub type Made = Option<(&'static str, Value)>;

/// A number from a seed, the same on every run.
pub fn roll(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// One driver's day, decided before any source writes it down.
pub struct Day {
    /// Minutes after the DSP's midnight.
    pub departed: i64,
    pub stops: i64,
    pub missed: i64,
    /// The meal Cortex records, if one was taken: its start, in minutes after midnight.
    pub meal: Option<i64>,
    /// Whether Paycom has the lunch punched; it may not match Cortex.
    pub lunch_punched: bool,
    pub length: i64,
    pub inspection_seconds: i64,
}
impl Day {
    pub fn last_stop(&self) -> i64 {
        self.departed + self.length
    }
    pub fn clock_in(&self) -> i64 {
        self.departed - 40
    }
    pub fn clock_out(&self) -> i64 {
        self.last_stop() + 55
    }
}

pub fn plan(driver: u64, day: i64) -> Option<Day> {
    // Two days off a week, in a pattern of each driver's own.
    let off = (day + driver as i64) % 7 == 0 || (day + 2 * driver as i64) % 7 == 3;
    if off {
        return None;
    }
    let r = roll(driver * 1000 + day as u64);
    let stops = 105 + (r % 50) as i64;
    let missed = if (r >> 8).is_multiple_of(4) {
        1 + ((r >> 12) % 3) as i64
    } else {
        0
    };
    let departed = 9 * 60 + 30 + ((r >> 16) % 45) as i64;
    let length = 400 + ((r >> 20) % 120) as i64;
    let meal = (!(r >> 28).is_multiple_of(9)).then_some(departed + length / 2);
    let short = (r >> 40).is_multiple_of(10);
    Some(Day {
        departed,
        stops,
        missed,
        meal,
        lunch_punched: meal.is_some() || (r >> 36).is_multiple_of(2),
        length,
        inspection_seconds: if short {
            30 + ((r >> 44) % 50) as i64
        } else {
            120 + ((r >> 44) % 300) as i64
        },
    })
}

pub fn hhmm(minutes: i64) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// How many packages a stop holds, the same on every run.
pub fn packages_at(driver: u64, day: i64, stop: i64) -> i64 {
    1 + (roll((driver << 32) + ((day as u64) << 16) + stop as u64) % 2) as i64
}
/// Amazon's reason a stop's packages came back, or where they were left.
pub fn context_at(driver: u64, day: i64, stop: i64, missed: bool) -> &'static str {
    let why = roll(((day as u64) << 24) + (driver << 12) + stop as u64);
    if missed {
        [
            "BUSINESS_CLOSED",
            "OBJECT_MISSING",
            "DAMAGED",
            "INACCESSIBLE_DELIVERY_LOCATION",
            "ADDRESS_NOT_FOUND",
        ][(why % 5) as usize]
    } else {
        [
            "DELIVERED_TO_DOORSTEP",
            "DELIVERED_TO_DOORSTEP",
            "DELIVERED_TO_DOORSTEP",
            "DELIVERED_TO_SAFE_LOCATION",
            "DELIVERED_TO_HOUSEHOLD_MEMBER",
        ][(why % 5) as usize]
    }
}
pub fn tracking(date: NaiveDate, driver: u64, stop: i64, k: i64) -> String {
    format!(
        "TBA{:04}{:02}{:03}{}",
        chrono::Datelike::ordinal(&date),
        driver,
        stop,
        k
    )
}

/// The synthetic DSP as each step fills it.
pub struct World {
    pub dsp: String,
    pub timezone: String,
    zone: chrono_tz::Tz,
    /// Today where the DSP is.
    pub today: NaiveDate,
    /// The days filled, ending with yesterday.
    pub dates: Vec<NaiveDate>,
    /// Its people as their roster lists them, the dispatcher first.
    pub employees: Vec<Value>,
    /// Everyone else, who drives: their place in the roster, their roster code and name.
    pub drivers: Vec<(u64, String, String)>,
    /// Each day's collection scope, as the first step to queue a collection of that day
    /// leaves it for the steps that collect beside it.
    pub scopes: BTreeMap<String, Value>,
}
impl World {
    /// A driver's Amazon transporter ID.
    pub fn transporter(&self, driver: u64) -> String {
        format!("A{:02}SYNTH{:02}", driver * 7 % 97, driver)
    }
    /// Minutes after a day's midnight where the DSP is, as epoch milliseconds.
    pub fn at(&self, date: NaiveDate, minutes: i64) -> i64 {
        self.zone
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .map(|midnight| midnight.timestamp_millis() + minutes * 60_000)
            .unwrap_or_default()
    }
    /// Marks a collector's connection ready as the fixture's, so collections can be queued
    /// through it.
    pub fn connect(&self, db: &Store, provider: Provider) -> Result<()> {
        db.collector(&self.dsp, provider)?.exec(
            "UPDATE connections SET enabled=1,status='ready',account_label='fixture'",
            [],
        )?;
        Ok(())
    }
    /// Marks a job whose collection a step published itself as succeeded, as running it
    /// would have left it.
    pub fn collected(&self, db: &Store, job: &str) -> Result<()> {
        db.jobs
            .exec("UPDATE jobs SET status='succeeded' WHERE id=?", [job])?;
        Ok(())
    }
}

/// Fills Northline with two weeks of what every feature holds for its demo people, step
/// by step in the steps' order. Returns what it made.
pub fn seed(db: &Store) -> Result<Value> {
    ensure(
        db.config.development && db.config.fixture,
        "fixtures_require_development",
        403,
    )?;
    let dsp = db
        .platform
        .one(
            "SELECT id,timezone FROM dsps WHERE name='Northline Logistics'",
            [],
        )?
        .ok_or_else(|| Error::new("seed_required", 409))?;
    let (id, timezone) = (s(&dsp, "id").to_owned(), s(&dsp, "timezone").to_owned());
    let zone: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    db.enable_all_features(&id)?;
    db.set_profile(
        &id,
        json!({"stationCode": STATION, "abbreviation": "NLOG", "setupRequired": false}),
    )?;
    let today = chrono::Utc::now().with_timezone(&zone).date_naive();
    let dates: Vec<NaiveDate> = (1..=DAYS)
        .rev()
        .map(|back| today - Duration::days(back))
        .collect();
    let features = || registry().features.iter().map(|feature| &feature.mcp);
    let employees = match features().find_map(|mcp| mcp.synthetic.people) {
        Some(people) => people(&timezone)?,
        None => vec![],
    };
    let drivers: Vec<(u64, String, String)> = employees
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, e)| (i as u64, s(e, "code").to_owned(), s(e, "name").to_owned()))
        .collect();
    let (first, last) = (dates[0].to_string(), dates[dates.len() - 1].to_string());
    let mut world = World {
        dsp: id.clone(),
        timezone,
        zone,
        today,
        dates,
        employees,
        drivers,
        scopes: BTreeMap::new(),
    };
    let mut steps: Vec<&Step> = features().flat_map(|mcp| mcp.synthetic.steps).collect();
    steps.sort_by_key(|step| step.order);
    let mut made = json!({"dsp": id, "from": first, "to": last});
    for step in steps {
        if let Some((name, value)) = (step.run)(db, &mut world)? {
            made[name] = value;
        }
    }
    Ok(made)
}
