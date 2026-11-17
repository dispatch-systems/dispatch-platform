//! Legacy Tracker's endpoints, each registered with its path and access. The server checks the
//! session and the permission before a handler runs.
use super::types::DspCjf87gqdLegacyTrackerSummary;
use crate::backend::settings;
use dispatch_core::{
    Result,
    db::Store,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read},
    },
};

/// Those who use Conduct, whose page its link is on.
const USE: Dsp = Dsp("conduct.use");

pub fn routes() -> Vec<Route> {
    vec![read("/api/dsp/cjf87gqd-legacy-tracker", USE, summary)]
}

fn summary(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&DspCjf87gqdLegacyTrackerSummary {
        link: settings::link(db, c.dsp_id())?,
    })
}
