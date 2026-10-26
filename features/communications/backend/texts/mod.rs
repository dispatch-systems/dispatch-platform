//! Texting: the texts the DSP's dispatch phones relay, through whatever offers it `texting`
//! (Phones), read by the page beside the Slack conversations and answered by text. A work phone's
//! texts are a driver's, across days, which Today's Routes shows; any other number's are
//! Messaging's: the number's conversation, with whoever Driver Match has the number for, else the
//! name the DSP saved for it, else the number itself.
//!
//! - `storage`: conversations by text, each text filed in one, how far Phones' changes were read,
//!   and the names and blocks the DSP keeps for numbers.
//! - `filing`: filing each text Phones relays in its conversation, and a day's again as who held
//!   its work phones settles.
//! - `numbers`: one number written one way, how one reads, and what the DSP keeps of them.
//! - `reading`: the conversations and their texts, as the page reads them.
//! - `reply`: answering by text, and sending again one that failed.
//! - `contacts`: whom the DSP can text, and what someone at the DSP does with a number: saving a
//!   name for it, blocking it, texting it first.
//! - `live_day`: the day live route data holds, which Today's Routes lists and work conversations
//!   wait on.
//! - `todays_routes`: Today's Routes, the drivers on a route today with their work conversations,
//!   and the work phones nobody held.
//! - `picking`: Pick driver, filing a work phone nobody held under one of today's drivers.
//! - `elsewhere`: a person's conversations of each kind, which the page goes between.
//! - `workdays`: each workday's route and van, for the line that begins it in a driver's
//!   conversation.
pub(crate) mod contacts;
pub(crate) mod elsewhere;
pub(crate) mod filing;
pub(crate) mod live_day;
pub(crate) mod numbers;
pub(crate) mod picking;
pub(crate) mod reading;
pub(crate) mod reply;
pub(crate) mod storage;
pub(crate) mod todays_routes;
pub(crate) mod workdays;

use crate::api::types::DispatchPhone;
use chrono::{Duration, NaiveDate, NaiveTime, TimeZone};
use chrono_tz::Tz;
use dispatch_core::{Error, Result, db::Store};
use dispatch_phones_contract::{self as texting, Texting};

/// The minutes in a day: how far past its midnight a day may begin, as one clocks skip whole.
const DAY_MINUTES: i64 = 24 * 60;

/// Whether the DSP texts: something offers it `texting`.
pub fn offered(db: &Store, dsp: &str) -> Result<bool> {
    Ok(texting::for_dsp(db, dsp)?.is_some())
}
/// The DSP's dispatch phones, while it texts, and whether each is online.
pub fn phones(db: &Store, dsp: &str) -> Result<Vec<DispatchPhone>> {
    let Some(texting) = texting::for_dsp(db, dsp)? else {
        return Ok(Vec::new());
    };
    Ok(texting
        .phones(db, dsp)?
        .into_iter()
        .map(|phone| DispatchPhone {
            id: phone.id,
            name: phone.name,
            online: phone.online,
            last_seen_at: phone.last_seen_at,
        })
        .collect())
}
/// What offers the DSP texting, refused while nothing does.
fn offering(db: &Store, dsp: &str) -> Result<&'static dyn Texting> {
    texting::for_dsp(db, dsp)?.ok_or_else(|| Error::new("communications_texting_off", 409))
}
/// The DSP's own timezone, which its days are in.
pub fn zone(db: &Store, dsp: &str) -> Result<Tz> {
    db.find_dsp(dsp)?
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))
}
/// When `day` began in `zone`, in milliseconds: its first instant, at midnight, or where clocks
/// went forward over midnight, at the first minute they showed after; a day they skipped whole
/// began as the next did.
pub fn began(zone: Tz, day: NaiveDate) -> Result<i64> {
    let midnight = day.and_time(NaiveTime::MIN);
    (0..=DAY_MINUTES)
        .find_map(|minute| {
            zone.from_local_datetime(&(midnight + Duration::minutes(minute)))
                .earliest()
        })
        .map(|began| began.timestamp_millis())
        .ok_or_else(|| Error::new("invalid_date", 400))
}
