//! Live Route Data's settings, on its own page of a DSP's Settings, which draws them from this
//! declaration alone.
use dispatch_core::{
    Result,
    db::Store,
    ensure,
    manifest::{Section, Window, section, time, toggle},
};
use serde::Deserialize;
use serde_json::Value;

/// When Live Route Data reads a DSP's routes each day, where the DSP is, and whether the day is
/// saved into historical route data as it stops, for those holding `live_route_data.manage`. Its
/// id and its fields' are permanent once shipped: each DSP's values are kept under them.
pub const COLLECTION: Section = section(
    "live_route_data.collection",
    "Collection",
    "live_route_data.manage",
    &[
        time("start", "Start", "08:00"),
        time("stop", "Stop", "21:00"),
        toggle("save_to_history", "Save to historical route data", true),
    ],
)
.saved(within_a_day);

/// `COLLECTION`'s values, each field by its id: its default until the DSP saves one.
#[derive(Deserialize)]
pub struct Collection {
    pub start: String,
    pub stop: String,
    /// Whether the day is saved into historical route data as it stops.
    pub save_to_history: bool,
}
impl Collection {
    /// When the DSP's routes are read each day, as the live readers keep it.
    pub fn window(&self) -> Result<Window> {
        Window::parse(&self.start, &self.stop)
    }
}

/// The DSP's `COLLECTION`, as Live Route Data reads it: one keyed lookup, of the DSP the caller
/// was authorized for.
pub(crate) fn collection(db: &Store, dsp: &str) -> Result<Collection> {
    dispatch_core::settings::read(db, dsp, &COLLECTION)
}

/// Refuses, in the save's own write, a window whose stop is no later than its start
/// (`live_route_data_window_invalid`): a live day is one calendar day where the DSP is, so its
/// window never runs past midnight, and every read of it is a read of today.
fn within_a_day(_: &Store, _: &str, values: &Value) -> Result<()> {
    let window = Collection::deserialize(values)?.window()?;
    ensure(
        window.stop > window.start,
        "live_route_data_window_invalid",
        400,
    )
}

#[cfg(test)]
#[path = "../tests/backend/settings.rs"]
mod tests;
