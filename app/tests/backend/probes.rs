//! The collectors' live probes that measure a collection together with the feature that
//! keeps it, so need the app's whole registry. Ignored: an operator runs one at a time
//! against a copy of a DSP, with the `operator-probes` feature. Never run by CI.
use dispatch_core::Result;

#[tokio::test]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn measure_live_collection() -> Result<()> {
    dispatch_paycom::measure_live_collection().await
}

#[tokio::test]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn measure_route_method() -> Result<()> {
    crate::browsers::cortex::probes::measure_route_method(crate::routedata::prepare).await
}
