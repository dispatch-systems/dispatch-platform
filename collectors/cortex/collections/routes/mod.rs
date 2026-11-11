//! Routes: a station's day of itineraries, as the execution pages read them.
mod browser;
mod capture;
pub(crate) mod collect;
mod native;
pub(crate) mod page;
pub(crate) mod protocol;
mod signed;

pub use browser::Browser;
pub use capture::{
    Capture, Collection, ItineraryCapture, JOB_KIND, MAX_BODY, MAX_CAPTURE_BYTES, MAX_ITINERARIES,
    Mode, Request, add_capture_bytes, fixture, listed, token,
};
pub use native::{Check, Failure, Native};
use serde::{Deserialize, Serialize};

/// How a day's itineraries are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Navigation {
    /// Each itinerary's own page is loaded.
    Reload,
    /// The list is loaded once per tab and the application moves between itineraries.
    InApp,
    /// The list's application signs every read, which is downloaded; what that does not
    /// read is read as `InApp` does.
    Signed,
    /// Every read is signed here, with no page; a day that does not read whole is read as
    /// `Signed` reads it.
    Native,
}
/// How a day is read. Native reads avoid browser signing and rendering, with bounded
/// downloads. Compare the alternatives with the `measure_route_method` operator probe.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Method {
    pub navigation: Navigation,
    /// Tabs reading itineraries at once.
    pub tabs: usize,
    /// Extra tabs each get a window of their own rather than a background tab. A
    /// withheld response needs no window: nothing is rendered.
    pub windows: bool,
    /// A captured response is failed for the page, which then never parses or renders
    /// it.
    pub withhold: bool,
    /// Reads the application signs downloaded at once.
    pub downloads: usize,
}
impl Default for Method {
    fn default() -> Self {
        Self {
            navigation: Navigation::Native,
            tabs: 3,
            windows: false,
            withhold: true,
            downloads: 12,
        }
    }
}
impl Method {
    /// How days were read before in-app moves: a page load per itinerary, two windows.
    #[cfg(feature = "operator-probes")]
    pub(crate) fn reload() -> Self {
        Self {
            navigation: Navigation::Reload,
            tabs: 2,
            windows: true,
            withhold: false,
            ..Self::default()
        }
    }
}

/// What a job's metrics call a day read natively.
pub const NATIVE: &str = "routes_native";
/// Why a day read whole natively was not kept: it failed the collection's checks.
pub const INVALID: &str = "routes_native_invalid";
/// A day read with a browser, and how.
pub struct Day {
    pub capture: Capture,
    /// What read it, as a job's metrics name it: `routes_native`, `routes_signed` or
    /// `routes_signed_moved`. None in fixture mode, as a fixture job names none.
    pub method: Option<&'static str>,
    /// Why it was not read natively, when it was read another way: `Failure::label`, or
    /// `routes_native_invalid` for a day read whole that failed the collection's checks.
    pub fallback: Option<&'static str>,
    /// A reader to read the next with no browser (`Native::read`): the one that read the day
    /// natively, with the key it found; read another way, one carrying the browser's session as
    /// the read left it (`Browser::read`). None in fixture mode, and for a job, which reads
    /// no next.
    pub native: Option<Native>,
}
