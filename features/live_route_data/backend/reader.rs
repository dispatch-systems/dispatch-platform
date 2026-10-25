//! The live reader: today's routes at a DSP's station, read every five minutes inside the window
//! its settings set, and kept as live route data.
//!
//! A read goes natively, with no browser, whenever the DSP's reader holds one a browser handed
//! back; otherwise, or once that reader fails, it opens the DSP's Cortex browser, reads the day
//! as the routes collection does, and keeps the native reader the browser hands back for the
//! next. Cortex briefly unavailable fails a read with no browser, and only a few in a row open
//! one. A reader that fails on its first read after a browser handed it back leaves the next
//! reads to the browser for a while, so a native reading that keeps failing doesn't add a
//! failed read to every browser's. A read is kept only whole.
use super::settings;
use dispatch_core::{
    Result,
    collection::{browser::Pending, readers::Reading},
    db::Store,
    manifest::{Counts, LiveReader, Window},
};
use dispatch_cortex::{
    self as cortex,
    discovery::Scope,
    routes::{self, Browser, Capture, Day, Failure, Native, Request},
};
use std::time::Duration;

/// Native reads in a row that may find Cortex unavailable, each failing without a browser,
/// before the next opens the browser anyway.
const UNAVAILABLE_READS: u32 = 2;
/// Reads that open the browser, once a reader a browser handed back fails its first read: that
/// read, and the next with no native read first. The read after them tries native again, as the
/// experiment's ladder tried native again every sixth poll.
const BROWSER_READS: u32 = 5;

/// Reads a DSP's routes live (`live_route_data.routes`).
pub struct Routes;
pub static READER: Routes = Routes;

/// What the reader keeps in memory for a DSP between its reads: the native reader the last
/// browser handed back, with the scope it reads, and when the next read goes to the browser.
#[derive(Default)]
pub struct Held {
    native: Option<(Native, Scope)>,
    ladder: Ladder,
}

/// When a read goes to the browser rather than natively.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Ladder {
    /// Native reads in a row that found Cortex unavailable.
    unavailable: u32,
    /// Whether the native reader came from the last browser and hasn't read since.
    fresh: bool,
    /// Reads still to open the browser without trying native first.
    browser_reads: u32,
}
impl Ladder {
    /// Whether this read tries the native reader first, when it holds one.
    fn native_first(&self) -> bool {
        self.browser_reads == 0
    }
    /// The native reader read the day whole.
    fn read_natively(&mut self) {
        self.unavailable = 0;
        self.fresh = false;
    }
    /// The native reader found Cortex unavailable: whether the read fails with no browser.
    fn unavailable(&mut self) -> bool {
        self.unavailable += 1;
        self.unavailable <= UNAVAILABLE_READS
    }
    /// The native reader failed otherwise: one fresh from a browser leaves the next reads to it.
    fn failed(&mut self) {
        if self.fresh {
            self.browser_reads = BROWSER_READS;
        }
    }
    /// A read opens the browser.
    fn browsing(&mut self) {
        self.browser_reads = self.browser_reads.saturating_sub(1);
    }
    /// The browser read the day and handed back a native reader.
    fn browsed(&mut self) {
        self.unavailable = 0;
        self.fresh = true;
    }
}

/// One whole read of today, and how it was read.
pub struct Read {
    capture: Capture,
    /// Whether it opened the DSP's browser.
    browser: bool,
    /// What read it and why native reading fell back, as a job's metrics name them.
    labels: Vec<&'static str>,
}

impl LiveReader for Routes {
    type Held = Held;
    type Read = Read;
    fn id(&self) -> &'static str {
        "live_route_data.routes"
    }
    fn every(&self) -> Duration {
        Duration::from_secs(5 * 60)
    }
    /// Its settings' window, while Routes keeps the DSP's route data at a station: none else.
    fn window(&self, db: &Store, dsp: &str) -> Result<Option<Window>> {
        let routes = db.feature_enabled(dsp, dispatch_routes::FEATURE.switch.id)?;
        if !routes || db.profile(dsp)?.station_code.is_empty() {
            return Ok(None);
        }
        settings::collection(db, dsp)?.window().map(Some)
    }
    fn read<'a>(&'a self, reading: &'a Reading, held: &'a mut Held) -> Pending<'a, Read> {
        Box::pin(async move {
            let dsp = reading.dsp.clone();
            let request = reading
                .state
                .read(move |db| dispatch_routes::live_request(db, &dsp))
                .await?;
            let mut labels = Vec::new();
            // The native reader goes first while it reads the scope today's request names.
            let native = held.native.take().filter(|(_, scope)| {
                held.ladder.native_first() && request.scope_request().validate_scope(scope).is_ok()
            });
            if let Some((mut native, scope)) = native {
                let fallback = match native.read(&scope, request.mode).await {
                    Ok(capture) => match capture.validate(&request) {
                        Ok(()) => {
                            held.ladder.read_natively();
                            held.native = Some((native, scope));
                            return Ok(Read {
                                capture,
                                browser: false,
                                labels: vec![routes::NATIVE],
                            });
                        }
                        Err(_) => {
                            held.ladder.failed();
                            routes::INVALID
                        }
                    },
                    Err(failure) => {
                        let label = failure.label();
                        match failure {
                            // Cortex briefly unavailable keeps the reader for the next read.
                            Failure::Unavailable(error) if held.ladder.unavailable() => {
                                held.native = Some((native, scope));
                                return Err(error);
                            }
                            Failure::Unavailable(_) => {}
                            _ => held.ladder.failed(),
                        }
                        label
                    }
                };
                labels.push(fallback);
            }
            held.ladder.browsing();
            let (day, scope) = browse(reading, &request).await?;
            held.ladder.browsed();
            held.native = day.native.map(|native| (native, scope));
            labels.extend(day.fallback);
            labels.dedup();
            labels.extend(day.method);
            Ok(Read {
                capture: day.capture,
                browser: true,
                labels,
            })
        })
    }
    /// A day's first read empties live route data of the day before.
    fn started<'a>(&'a self, reading: &'a Reading, _: &'a Read) -> Pending<'a, ()> {
        Box::pin(dispatch_routes::clear_live(&reading.state, &reading.dsp))
    }
    fn kept<'a>(&'a self, reading: &'a Reading, read: Read) -> Pending<'a, Counts> {
        Box::pin(async move {
            let kept =
                dispatch_routes::keep_live(&reading.state, &reading.dsp, read.capture).await?;
            let mut counts = kept.counts();
            if read.browser {
                counts.push(("browser", 1));
            }
            counts.extend(read.labels.into_iter().map(|label| (label, 1)));
            Ok(counts)
        })
    }
    /// The day is saved into historical route data if the DSP chose so; live route data keeps
    /// it either way, until the next day starts.
    fn stopped<'a>(&'a self, reading: &'a Reading) -> Pending<'a, ()> {
        Box::pin(async move {
            let dsp = reading.dsp.clone();
            let collection = reading
                .state
                .read(move |db| settings::collection(db, &dsp))
                .await?;
            if collection.save_to_history {
                dispatch_routes::save_live_day(&reading.state, &reading.dsp).await?;
            }
            Ok(())
        })
    }
}

/// Today's routes read in the DSP's Cortex browser, and the scope they were read in. The browser
/// is the reader's alone while it reads, and closes after.
async fn browse(reading: &Reading, request: &Request) -> Result<(Day, Scope)> {
    let open = reading.browser(cortex::PROVIDER).await?;
    let read = async {
        let mut browser = Browser::new(open.browser().await, &reading.state.config)?;
        let scope = browser.scope(request).await?;
        let day = browser.read(request, &scope).await?;
        Ok((day, scope))
    }
    .await;
    open.close().await;
    read
}

#[cfg(test)]
#[path = "../tests/backend/reader.rs"]
mod tests;
