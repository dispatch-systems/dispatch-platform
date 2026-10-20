//! Reads the fleet over plain HTTP with the browser's session. Its reads are written once
//! the fleet page's own requests are known.
use super::capture::{Capture, Request};
use crate::connection::Driver;
use dispatch_core::{Error, Result, collection::browser::Run};

impl Driver {
    pub(crate) async fn collect_vans(&mut self, request: &Request, _run: &Run<'_>) -> Result<Capture> {
        request.validate()?;
        Err(Error::new("vans_unavailable", 501))
    }
}
