//! GIFs of KLIPY's, a part of Communications switched on its own (`PART`). The page searches
//! KLIPY from the browser and shows each GIF from KLIPY's addresses, as KLIPY's terms have it;
//! Dispatch keeps only a message's GIF's addresses, never the GIF. How one goes to the person
//! messaged is `SENDING`.
use crate::api::types::{GifSearch, GifVersion, MessageGif};
use dispatch_core::{
    Error, Result,
    db::{Db, migrations::add_column},
    ensure,
    foundation::{config::Config, crypto},
    manifest::Loads,
};
use serde_json::{Value, json};

/// The part of Communications that chooses GIFs: off, the page offers none, and none is sent.
pub const PART: &str = "communications.gifs";

/// The server's KLIPY API key, in its settings; none, the page offers no GIFs.
pub const KEY: &str = "KLIPY_API_KEY";

/// How a GIF goes to the person messaged. `File`: the GIF itself, which a phone fetches from KLIPY
/// as it sends the text, and Slack shows as an image. `Link`: KLIPY's address of it, after what
/// was written. Either way the page shows the GIF.
pub const SENDING: Sending = Sending::File;

/// How a GIF goes to the person messaged (`SENDING`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sending {
    File,
    Link,
}

/// The first `versionCode` of DSP Dispatch: Relay that sends a GIF, fetching it from KLIPY: `main`'s
/// commit count at the change that taught it to (#852). A text with a GIF, sent as a file, fails on a
/// phone running an older one, which would send its words alone (`OUTDATED`).
pub const RELAY_SENDS_GIFS: i64 = 1546;

/// Why a text with a GIF failed on a phone whose app is older than `RELAY_SENDS_GIFS`.
pub const OUTDATED: &str = "phones_app_outdated";

/// KLIPY's content filter for what the page finds: `off`, `low`, `medium` or `high`.
const FILTER: &str = "medium";

/// The largest size Slack is given, and a link names: the largest version at most this, else the
/// smallest.
const SHARED_MOST: u64 = 2 * 1024 * 1024;

/// Where fixture mode's stand-in for KLIPY answers, on the server itself.
const FIXTURE: &str = "/api/dispatch-phone/gifs/fixture";

/// Where the page searches KLIPY's GIFs for the member `user` of the DSP `dsp`: refused while the
/// server has no KLIPY key.
pub fn search(config: &Config, dsp: &str, user: &str) -> Result<GifSearch> {
    let address = if config.fixture {
        FIXTURE.to_owned()
    } else {
        let key = config
            .setting(KEY)
            .ok_or_else(|| Error::new("communications_gifs_unavailable", 503))?;
        format!("https://api.klipy.com/api/v1/{key}")
    };
    Ok(GifSearch {
        address,
        filter: FILTER.to_owned(),
        // The same member each time, which KLIPY can't tell who is.
        customer: crypto::sha(format!("{dsp}:{user}"))[..32].to_owned(),
    })
}

/// `gif`, as the page sent it, when it's one of KLIPY's: from its addresses alone (fixture mode's
/// stand-in's, there), a few sizes of one GIF, each within what a GIF may be.
pub fn checked(gif: MessageGif, fixture: bool) -> Result<MessageGif> {
    let named = |text: &str, most: usize| !text.trim().is_empty() && text.chars().count() <= most;
    let klipy = |url: &str| {
        if fixture {
            return url.starts_with(&format!("{FIXTURE}/media/"));
        }
        crate::FEATURE
            .page_sources
            .iter()
            .filter(|source| source.loads == Loads::Images)
            .any(|source| url.starts_with(&format!("{}/", source.origin)))
    };
    ensure(
        named(&gif.slug, 200)
            && gif.title.chars().count() <= 200
            && (1..=VERSIONS).contains(&gif.versions.len())
            && gif.versions.iter().all(|version| {
                named(&version.url, 2_000)
                    && klipy(&version.url)
                    && !version.url.contains(char::is_whitespace)
                    && (1..=SIDE_MOST).contains(&version.width)
                    && (1..=SIDE_MOST).contains(&version.height)
                    && (1..=SIZE_MOST).contains(&version.size)
            }),
        "invalid_input",
        400,
    )?;
    Ok(gif)
}

/// How many sizes a GIF comes in, at most: KLIPY's hd, md, sm and xs.
const VERSIONS: usize = 4;
const SIDE_MOST: u32 = 4096;
const SIZE_MOST: u64 = 50 * 1024 * 1024;

/// The size of `gif` Slack is given, and a link names: the largest at most `SHARED_MOST`, else
/// the smallest.
pub fn shared(gif: &MessageGif) -> &GifVersion {
    let by_size = || gif.versions.iter();
    by_size()
        .filter(|version| version.size <= SHARED_MOST)
        .max_by_key(|version| version.size)
        .or_else(|| by_size().min_by_key(|version| version.size))
        .expect("a GIF comes in one size at least")
}

/// What's written, with KLIPY's address of `gif` after it, as a link sends it.
pub fn linked(written: &str, gif: &MessageGif) -> String {
    let url = &shared(gif).url;
    if written.is_empty() {
        url.clone()
    } else {
        format!("{written}\n{url}")
    }
}

/// `gif` as its column keeps it.
pub fn column(gif: Option<&MessageGif>) -> Result<Option<String>> {
    Ok(gif.map(serde_json::to_string).transpose()?)
}

/// The GIF its column keeps, if any.
pub fn of_column(kept: Option<String>) -> Result<Option<MessageGif>> {
    Ok(kept.as_deref().map(serde_json::from_str).transpose()?)
}

/// Communications' migration 19: the GIF a message carries, as its column keeps it: one sent in
/// Slack, one noted as it goes there, and a text's.
pub(crate) fn add_gifs(db: &Db) -> Result<()> {
    for table in [
        "communications_messages",
        "communications_replies",
        "phones_texts",
    ] {
        add_column(db, table, "gif", "TEXT")?;
    }
    Ok(())
}

/// Fixture mode's stand-in for KLIPY's trending GIFs, or its search for `q`: as KLIPY answers,
/// each GIF at the server's stand-in addresses.
pub fn fixture_results(q: Option<&str>) -> Value {
    let wanted = q.map(str::to_lowercase).unwrap_or_default();
    let gifs: Vec<Value> = FIXTURE_GIFS
        .iter()
        .filter(|(_, title, _)| title.to_lowercase().contains(wanted.trim()))
        .map(|(slug, title, tall)| {
            // Each size of it the same shape, as wide as KLIPY's are.
            let size = |name: &str, width: u32, bytes: u64| {
                json!({ "gif": {
                    "url": format!("{FIXTURE}/media/{slug}-{name}.gif"),
                    "width": width, "height": width * tall / 100, "size": bytes,
                } })
            };
            json!({
                "id": slug, "slug": slug, "title": title, "type": "gif",
                "file": {
                    "hd": size("hd", 498, 2_600_000),
                    "md": size("md", 498, 1_400_000),
                    "sm": size("sm", 220, 206_000),
                    "xs": size("xs", 90, 64_000),
                },
            })
        })
        .collect();
    json!({ "result": true, "data": {
        "data": gifs, "current_page": 1, "per_page": gifs.len(), "has_next": false,
    } })
}

/// Fixture mode's stand-in for a GIF of KLIPY's: one pixel, each the same.
pub const FIXTURE_GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xf6\xc9\x45\x00\x00\x00\
\x21\xf9\x04\x00\x00\x00\x00\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b";

/// The stand-in's GIFs, as many as a page of KLIPY's: each by its slug, its title, and how tall it
/// is for its width, in hundredths, as KLIPY's come in many shapes.
const FIXTURE_GIFS: &[(&str, &str, u32)] = &[
    ("thumbs-up", "Thumbs up", 56),
    ("thank-you", "Thank you", 100),
    ("nice-work", "Nice work", 75),
    ("on-my-way", "On my way", 133),
    ("lets-go", "Let's go", 56),
    ("high-five", "High five", 90),
    ("well-done", "Well done", 60),
    ("good-morning", "Good morning", 100),
    ("great-job", "Great job", 75),
    ("see-you", "See you", 56),
    ("you-got-this", "You got this", 120),
    ("drive-safe", "Drive safe", 66),
    ("awesome", "Awesome", 56),
    ("on-it", "On it", 100),
    ("keep-going", "Keep going", 75),
    ("almost-there", "Almost there", 56),
    ("all-done", "All done", 133),
    ("happy-friday", "Happy Friday", 70),
    ("cheers", "Cheers", 56),
    ("no-worries", "No worries", 100),
    ("okay", "Okay", 80),
    ("yes", "Yes", 56),
    ("amazing", "Amazing", 110),
    ("lets-roll", "Let's roll", 56),
];

#[cfg(test)]
#[path = "../tests/backend/gifs.rs"]
mod tests;
