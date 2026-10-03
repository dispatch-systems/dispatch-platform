//! Operator-only observations of Cortex. Never run by CI or print provider records.
//! A probe that measures what the feature keeping a collection makes of it needs that
//! feature, so the app's tests run it and hand it the feature's part. The probes that
//! need Cortex alone are its ignored tests.
use crate::{connection::Driver, discovery::Scope};
use dispatch_core::{
    Error, Result,
    collection::{browser::browseros, metrics::Recorder},
    db::{self, s},
    ensure,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicU64,
    time::Duration,
};
use tokio::time::Instant;

fn env_path(name: &str) -> Result<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| Error::new("benchmark_configuration_required", 400))
}
// Drop also stops the blocking sampler if a collection is cancelled or unwinds.
struct Sampling(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for Sampling {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
/// The cgroup this process runs in, read from `/proc/self/cgroup`.
fn own_cgroup() -> Option<PathBuf> {
    let text = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let path = text.lines().find_map(|line| line.strip_prefix("0::"))?;
    Some(PathBuf::from(format!("/sys/fs/cgroup{path}")))
}
fn cgroup_cpu_usec(cgroup: &Path) -> u64 {
    std::fs::read_to_string(cgroup.join("cpu.stat"))
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("usage_usec "))
                .and_then(|v| v.trim().parse().ok())
        })
        .unwrap_or(0)
}
/// The value with every array's items in a stable order, so two reads of the same
/// data compare equal however the provider ordered its lists.
fn canonical(value: &Value) -> Value {
    match value {
        // Counted from the moment of the request, so they differ between any two reads.
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !["timeRemainingSecs", "backendTime"].contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect(),
        ),
        Value::Array(items) => {
            let mut items: Vec<Value> = items.iter().map(canonical).collect();
            items.sort_by_cached_key(|v| v.to_string());
            Value::Array(items)
        }
        other => other.clone(),
    }
}
fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(text.as_bytes());
    hash.iter().take(8).map(|b| format!("{b:02x}")).collect()
}
fn own_rss() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("VmRSS:"))
                .and_then(|v| v.split_whitespace().next())
                .and_then(|v| v.parse::<u64>().ok())
        })
        .unwrap_or(0)
        * 1024
}

// One day's routes read by the method in DISPATCH_BENCHMARK_METHOD ("reload" for the
// original, or a method as JSON; the current default when unset), with the browser in
// DISPATCH_BENCHMARK_MODE (headless or windowed). Run each method in its own cgroup
// (systemd-run --user --scope) so CPU and peak memory cover this process and its
// browser. Prints times, CPU, memory, bytes through the egress proxy, and digests of
// what was read and of the rows it becomes; never a value.
// `prepare` is how the feature that keeps the routes shapes a day into its rows: what
// the probe times and digests. `sent` and `received` count the bytes through the egress
// proxy, which only test builds count.
pub async fn measure_route_method<R: serde::Serialize>(
    prepare: impl FnOnce(&crate::routes::Capture) -> Result<R>,
    sent: &AtomicU64,
    received: &AtomicU64,
) -> Result<()> {
    use crate::collections::routes::collect::Method;
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let method: Method = match std::env::var("DISPATCH_BENCHMARK_METHOD").as_deref() {
        Ok("reload") => Method::reload(),
        Ok(text) if !text.is_empty() => serde_json::from_str(text)?,
        _ => Method::default(),
    };
    let mode = if std::env::var("DISPATCH_BENCHMARK_MODE").as_deref() == Ok("headless") {
        browseros::Mode::Headless
    } else {
        browseros::Mode::Windowed
    };
    let scope: Scope = serde_json::from_str(
        &std::env::var("DISPATCH_BENCHMARK_SCOPE")
            .map_err(|_| Error::new("benchmark_configuration_required", 400))?,
    )?;
    let profile = dsp.join("state/browsers/cortex-browseros");
    let runtime = browseros::Runtime::new(
        Path::new("/opt/dispatch-browseros/0.50.5/browseros"),
        Path::new("/usr/local/libexec/dispatch-dev/bwrap"),
        &env_path("DISPATCH_BENCHMARK_WORKER")?,
        &env_path("DISPATCH_BENCHMARK_RUNS")?,
        1,
    )?;
    let cgroup = own_cgroup().ok_or_else(|| Error::new("benchmark_cgroup_required", 400))?;
    let launched = Instant::now();
    let browser = runtime
        .start(
            &profile,
            mode,
            browseros::NetworkPolicy::Hosts(&crate::BROWSER_HOSTS),
        )
        .await?;
    let mut driver = Driver::new(browser, &profile, None).await?;
    let result = async {
        let secrets = dsp.join("secrets");
        let credentials = dispatch_core::foundation::crypto::decrypt(
            &db::key_file(&secrets.join("vault.key"))?,
            &format!("{}:cortex:2", dsp.file_name().unwrap().to_str().unwrap()),
            &std::fs::read_to_string(secrets.join("cortex.enc"))?,
        )?;
        let signed = driver
            .request(json!({"action":"start","credentials":credentials}))
            .await;
        ensure(
            signed.is_ok_and(|v| v["type"] == "ready"),
            "benchmark_verification_required",
            409,
        )?;
        let signed_in_ms = launched.elapsed().as_millis();
        // Everything from here is the collection itself.
        let cpu_before = cgroup_cpu_usec(&cgroup);
        let mut peak = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(cgroup.join("memory.peak"))?;
        {
            use std::io::Write;
            let _ = peak.write_all(b"reset\n");
        }
        let (sent_before, received_before) = (
            sent.load(std::sync::atomic::Ordering::Relaxed),
            received.load(std::sync::atomic::Ordering::Relaxed),
        );
        let pid = driver.browser.process_id();
        let sampling = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let sampling_guard = Sampling(sampling.clone());
        let sampler = {
            let sampling = sampling.clone();
            tokio::task::spawn_blocking(move || {
                let (mut max_pss, mut sum_pss, mut samples, mut max_own) = (0u64, 0u64, 0u64, 0u64);
                while sampling.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Some(memory) = dispatch_core::collection::metrics::memory(pid) {
                        max_pss = max_pss.max(memory.pss);
                        sum_pss += memory.pss;
                        samples += 1;
                    }
                    max_own = max_own.max(own_rss());
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                (max_pss, sum_pss / samples.max(1), max_own)
            })
        };
        let request = crate::routes::Request {
            collection: crate::routes::Collection::Routes,
            mode: crate::routes::Mode::Final,
            date: scope.date.clone(),
            station: scope.station.clone(),
            timezone: scope.timezone.clone(),
            dsp_name: "Benchmark".into(),
            dsp_abbreviation: "BNCH".into(),
            service_area_id: Some(scope.service_area_id.clone()),
            provider: Some(scope.provider.clone()),
        };
        let metrics = Recorder::new(&json!({}));
        let started = Instant::now();
        let capture = tokio::time::timeout(Duration::from_secs(600),
            driver.collect_routes_with(&request, &method, &metrics, |_, _| async { Ok(()) }))
            .await.map_err(|_| Error::new("benchmark_deadline", 504)).and_then(|result| result);
        let collect_ms = started.elapsed().as_millis();
        let browser_cpu_usec = cgroup_cpu_usec(&cgroup) - cpu_before;
        let started = Instant::now();
        let prepared = capture.and_then(|capture| {
            prepare(&capture).map(|prepared| (capture, prepared))
        });
        let prepare_ms = started.elapsed().as_millis();
        drop(sampling_guard);
        let (max_pss, avg_pss, max_own) = sampler.await.unwrap_or((0, 0, 0));
        let (capture, prepared) = prepared?;
        let peak_bytes: u64 = {
            use std::io::{Read, Seek};
            let mut text = String::new();
            peak.seek(std::io::SeekFrom::Start(0))?;
            peak.read_to_string(&mut text)?;
            text.trim().parse().unwrap_or(0)
        };
        // What was read, key order aside, and the rows it becomes, raw blobs aside.
        let mut read: Vec<(String, String)> = capture
            .itineraries
            .iter()
            .map(|i| {
                let text = serde_json::from_str::<Value>(&i.detail)
                    .map(|v| digest(&canonical(&v).to_string()))
                    .unwrap_or_default();
                (i.id.clone(), text)
            })
            .collect();
        read.sort();
        let read: Vec<String> = read.into_iter().map(|(_, d)| d).collect();
        let mut list = capture.summaries.clone();
        if let Some(object) = list.as_object_mut() {
            object.remove("backendTime");
        }
        let mut routes = capture.route_summaries.clone();
        if let Some(object) = routes.as_object_mut() {
            object.remove("backendTime");
        }
        let prepared_rows: Vec<Value> = serde_json::to_value(&prepared)?
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut rows: Vec<(String, String)> = prepared_rows
            .iter()
            .map(|i| {
                let mut value = i.clone();
                if let Some(object) = value.as_object_mut() {
                    object.remove("raw");
                }
                (s(i, "id").to_owned(), digest(&canonical(&value).to_string()))
            })
            .collect();
        rows.sort();
        let rows: Vec<String> = rows.into_iter().map(|(_, d)| d).collect();
        let snapshot = serde_json::to_value(metrics.snapshot())?;
        let pages = &snapshot["pageReads"];
        let slowest: Vec<Value> = pages["slowest"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| p["elapsedMs"].clone())
            .collect();
        eprintln!(
            "BENCH {}",
            json!({
                "method": method,
                "mode": if matches!(mode, browseros::Mode::Headless) {"headless"} else {"windowed"},
                "signedInMs": signed_in_ms,
                "collectMs": collect_ms,
                "prepareMs": prepare_ms,
                "cpuSeconds": (browser_cpu_usec as f64 / 1e6),
                "cgroupPeakMB": peak_bytes / 1_048_576,
                "browserPeakPssMB": max_pss / 1_048_576,
                "browserAvgPssMB": avg_pss / 1_048_576,
                "processPeakRssMB": max_own / 1_048_576,
                "sentKB": (sent.load(std::sync::atomic::Ordering::Relaxed) - sent_before) / 1024,
                "receivedKB": (received.load(std::sync::atomic::Ordering::Relaxed) - received_before) / 1024,
                "itineraries": capture.itineraries.len(),
                "pages": {"completed": pages["completed"], "retries": pages["retries"], "direct": pages["direct"],
                    "totalMs": pages["totalMs"], "slowest": slowest},
                "detail": snapshot["detail"],
                "counts": {
                    "stops": prepared_rows.iter().map(|i| i["stops"].as_array().map_or(0, Vec::len)).sum::<usize>(),
                    "tasks": prepared_rows.iter().map(|i| i["tasks"].as_array().map_or(0, Vec::len)).sum::<usize>(),
                    "inactive": prepared_rows.iter().map(|i| i["inactive"].as_array().map_or(0, Vec::len)).sum::<usize>(),
                    "breaks": prepared_rows.iter().map(|i| i["breaks"].as_array().map_or(0, Vec::len)).sum::<usize>(),
                    "unknownStops": prepared_rows.iter().map(|i| i["unknownStops"].as_array().map_or(0, Vec::len)).sum::<usize>(),
                    "addresses": prepared_rows.iter().map(|i| i["addresses"].as_array().map_or(0, Vec::len)).sum::<usize>(),
                    "routes": capture.route_summaries["rmsRouteSummaries"].as_array().map_or(0, Vec::len),
                    "listed": capture.summaries["itinerarySummaries"].as_array().map_or(0, Vec::len),
                },
                "readDigest": digest(&read.join(",")),
                "rowsDigest": digest(&rows.join(",")),
                "listDigest": digest(&canonical(&list).to_string()),
                "routesDigest": digest(&canonical(&routes).to_string()),
                "read": read,
            })
        );
        Ok(())
    }
    .await;
    driver.browser.close().await;
    result
}

#[cfg(test)]
#[path = "../tests/backend/probes.rs"]
mod tests;
