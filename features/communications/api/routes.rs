//! Communications's endpoints, each registered with its path and access. The server checks the
//! session and the permission before a handler runs.
use super::types::CommunicationsSummary;
use crate::CommunicationsStore;
use dispatch_core::{
    Result,
    db::Store,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read},
    },
};

const VIEW: Dsp = Dsp("communications.view");

pub fn routes() -> Vec<Route> {
    vec![read("/api/dsp/communications", VIEW, summary)]
}

fn summary(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&CommunicationsSummary {
        dsp_id: c.dsp_id().to_owned(),
        items: db.communications_items(c.dsp_id())?.len(),
    })
}
