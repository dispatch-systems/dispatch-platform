//! One day's complete tenant scopes. A period queues at most one of these jobs per day,
//! keeping its backlog bounded independently of how many stations the DSP uses.
use super::Capture;
use crate::discovery::{CollectionRequest, Scope};
use dispatch_core::{Result, ensure, foundation::wire};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DayRequest {
    pub date: String,
    pub scope_requests: Vec<CollectionRequest>,
}
impl DayRequest {
    pub fn parse(value: &Value) -> Result<Option<Self>> {
        if value.get("scopeRequests").is_none() {
            return Ok(None);
        }
        let request: Self = wire::request(value)?;
        request.validate()?;
        Ok(Some(request))
    }
    pub fn validate(&self) -> Result<()> {
        ensure(!self.scope_requests.is_empty(), "invalid_cortex_scope", 400)?;
        let mut seen = HashSet::new();
        for request in &self.scope_requests {
            let scope = match request {
                CollectionRequest::Scoped(scope) => scope.clone(),
                CollectionRequest::Discover(discovery) => {
                    discovery.scope("discovery", "discovery")?
                }
            };
            scope.validate()?;
            ensure(
                scope.date == self.date
                    && scope.provider != "ALL_DRIVERS"
                    && seen.insert((scope.station, scope.service_area_id, scope.provider)),
                "invalid_cortex_scope",
                400,
            )?;
        }
        Ok(())
    }
    pub fn validate_scope(&self, scope: &Scope) -> Result<()> {
        ensure(
            scope.date == self.date
                && self
                    .scope_requests
                    .iter()
                    .any(|request| request.validate_scope(scope).is_ok()),
            "cortex_scope_mismatch",
            502,
        )
    }
    pub fn fixture(&self) -> Result<DayCapture> {
        self.validate()?;
        let captures = self
            .scope_requests
            .iter()
            .map(|request| {
                let scope = match request {
                    CollectionRequest::Scoped(scope) => scope.clone(),
                    CollectionRequest::Discover(discovery) => {
                        discovery.scope("area-demo", "provider-demo")?
                    }
                };
                Ok(super::fixture(&scope))
            })
            .collect::<Result<_>>()?;
        let day = DayCapture {
            date: self.date.clone(),
            captures,
        };
        day.validate(self)?;
        Ok(day)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DayCapture {
    pub date: String,
    pub captures: Vec<Capture>,
}
impl DayCapture {
    pub fn validate(&self, request: &DayRequest) -> Result<()> {
        request.validate()?;
        ensure(
            self.date == request.date && self.captures.len() == request.scope_requests.len(),
            "cortex_scope_mismatch",
            502,
        )?;
        let mut seen = HashSet::new();
        for (capture, expected) in self.captures.iter().zip(&request.scope_requests) {
            expected.validate_scope(&capture.scope)?;
            capture.validate(&capture.scope)?;
            ensure(
                seen.insert((
                    &capture.scope.station,
                    &capture.scope.service_area_id,
                    &capture.scope.provider,
                )),
                "cortex_scope_mismatch",
                502,
            )?;
        }
        Ok(())
    }
}
