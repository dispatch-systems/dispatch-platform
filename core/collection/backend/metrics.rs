//! Per-attempt diagnostics. Only timings, counts, memory totals and fixed failure
//! labels are persisted.
use crate::collection::api::metrics::{DocumentState, JobOutcome, PageRead, PageReads, PageStage};
pub use crate::collection::api::metrics::{JobMetrics as Metrics, JobPhase as Phase};
use crate::{
    Result,
    db::{self, Store, n, s},
};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    sync::{Arc, Mutex},
    time::Instant,
};

/// What a finished collection brought, as its collector counts it from its data.
#[derive(Default)]
pub struct Counts {
    pub employees: Option<usize>,
    pub timecards: Option<usize>,
    pub itineraries: Option<usize>,
    pub meals: Option<usize>,
    pub rows: Option<usize>,
}

/// The journal line for one attempt: outcome and timings, without page reads.
pub fn summary(metrics: &Metrics) -> Value {
    serde_json::json!({"outcome":metrics.outcome,"detail":metrics.detail,"queueMs":metrics.queue_ms,
        "elapsedMs":metrics.elapsed_ms,"authenticationMs":metrics.authentication_ms,"collectionMs":metrics.collection_ms})
}
impl Metrics {
    /// For a job given as its raw row in JSON; a worker starts from the typed row.
    pub fn new(job: &Value) -> Self {
        Self::begin(
            n(job, "attempt"),
            s(job, "started_at"),
            n(job, "available_at"),
        )
    }
    pub fn start(job: &crate::collection::api::jobs::JobRow) -> Self {
        Self::begin(
            job.attempt,
            job.started_at.as_deref().unwrap_or(""),
            job.available_at,
        )
    }
    fn begin(attempt: i64, started_at: &str, available_at: i64) -> Self {
        Self {
            attempt,
            started_at: started_at.into(),
            finished_at: None,
            outcome: JobOutcome::Running,
            error: None,
            phase: Some(Phase::Starting),
            detail: None,
            queue_ms: db::now().saturating_sub(available_at).max(0) as u64,
            elapsed_ms: 0,
            authentication_ms: None,
            verification_ms: None,
            collection_ms: None,
            publication_ms: None,
            employees: None,
            timecards: None,
            itineraries: None,
            meals: None,
            rows: None,
            peak_rss_bytes: None,
            peak_pss_bytes: None,
            peak_private_bytes: None,
            memory_samples: 0,
            incomplete_memory_samples: 0,
            page_reads: Some(PageReads::default()),
        }
    }
    fn page_reads_mut(&mut self) -> &mut PageReads {
        self.page_reads.get_or_insert_default()
    }
    fn add(&mut self, phase: Phase, ms: u64) {
        let value = match phase {
            Phase::Starting => return,
            Phase::Authentication => &mut self.authentication_ms,
            Phase::Verification => &mut self.verification_ms,
            Phase::Collection => &mut self.collection_ms,
            Phase::Publication => &mut self.publication_ms,
        };
        *value = Some(value.unwrap_or(0).saturating_add(ms));
    }
}
impl PageRead {
    fn add(&mut self, ms: u64) {
        match self.stage {
            PageStage::Navigation => self.navigation_ms += ms,
            PageStage::Content => self.content_ms += ms,
            PageStage::Extraction => self.extraction_ms += ms,
        }
    }
}
struct Clock {
    value: Metrics,
    started: Instant,
    changed: Instant,
    pages: Vec<(PageRead, Instant, Instant)>,
}
#[derive(Clone)]
pub struct Recorder(Arc<Mutex<Clock>>);
impl Recorder {
    pub fn new(job: &Value) -> Self {
        Self::of(Metrics::new(job))
    }
    pub fn start(job: &crate::collection::api::jobs::JobRow) -> Self {
        Self::of(Metrics::start(job))
    }
    fn of(value: Metrics) -> Self {
        let now = Instant::now();
        Self(Arc::new(Mutex::new(Clock {
            value,
            started: now,
            changed: now,
            pages: Vec::new(),
        })))
    }
    pub fn resumed(&self, count: usize) {
        self.0
            .lock()
            .expect("job metrics")
            .value
            .page_reads_mut()
            .resumed = count;
    }
    pub fn early_ready(&self) {
        self.0
            .lock()
            .expect("job metrics")
            .value
            .page_reads_mut()
            .early_ready += 1;
    }
    /// Timecards read from the provider's response instead of a rendered page.
    pub fn direct(&self) {
        self.0
            .lock()
            .expect("job metrics")
            .value
            .page_reads_mut()
            .direct += 1;
    }
    /// Response reads that a rendered read of the same employee confirmed.
    pub fn spot_checked(&self) {
        self.0
            .lock()
            .expect("job metrics")
            .value
            .page_reads_mut()
            .spot_checked += 1;
    }
    /// A response read that handed over to a rendered read is not a page failure.
    pub fn page_cancel(&self, ordinal: usize) {
        self.0
            .lock()
            .expect("job metrics")
            .pages
            .retain(|(page, _, _)| page.ordinal != ordinal);
    }
    pub fn page_start(&self, ordinal: usize, attempt: usize) {
        let mut clock = self.0.lock().expect("job metrics");
        if attempt > 1 {
            clock.value.page_reads_mut().retries += 1;
        }
        clock.pages.push((
            PageRead {
                ordinal,
                attempt,
                stage: PageStage::Navigation,
                elapsed_ms: 0,
                navigation_ms: 0,
                content_ms: 0,
                extraction_ms: 0,
                error: None,
                pending_requests: None,
                document_state: None,
            },
            Instant::now(),
            Instant::now(),
        ));
    }
    pub fn page_stage(&self, ordinal: usize, stage: &'static str) {
        let stage = PageStage::parse(stage).expect("known page stage");
        let mut clock = self.0.lock().expect("job metrics");
        if let Some((page, _, changed)) = clock
            .pages
            .iter_mut()
            .find(|(p, _, _)| p.ordinal == ordinal)
            && page.stage != stage
        {
            page.add(changed.elapsed().as_millis() as u64);
            page.stage = stage;
            *changed = Instant::now();
        }
    }
    pub fn page_loading(&self, ordinal: usize, pending: Option<usize>, state: &'static str) {
        let mut clock = self.0.lock().expect("job metrics");
        if let Some((page, _, _)) = clock
            .pages
            .iter_mut()
            .find(|(p, _, _)| p.ordinal == ordinal)
        {
            page.pending_requests = pending;
            page.document_state = Some(DocumentState::parse(state).expect("known document state"));
        }
    }
    pub fn page_finish(&self, ordinal: usize, error: Option<&str>) {
        let mut clock = self.0.lock().expect("job metrics");
        if let Some(index) = clock
            .pages
            .iter()
            .position(|(p, _, _)| p.ordinal == ordinal)
        {
            let (mut page, started, changed) = clock.pages.remove(index);
            page.add(changed.elapsed().as_millis() as u64);
            page.elapsed_ms = started.elapsed().as_millis() as u64;
            page.error = error.map(str::to_owned);
            let reads = clock.value.page_reads_mut();
            reads.total_ms += page.elapsed_ms;
            if error.is_some() {
                reads.failures.push(page.clone());
                if reads.failures.len() > 8 {
                    reads.failures.remove(0);
                }
            } else {
                reads.completed += 1;
                if page.attempt > 1 {
                    reads.recovered += 1;
                }
            }
            reads.slowest.push(page);
            reads
                .slowest
                .sort_by_key(|p| std::cmp::Reverse(p.elapsed_ms));
            reads.slowest.truncate(5);
        }
    }
    pub fn phase(&self, phase: Phase) {
        let mut clock = self.0.lock().expect("job metrics");
        if let Some(previous) = clock.value.phase {
            let elapsed = clock.changed.elapsed().as_millis() as u64;
            clock.value.add(previous, elapsed);
        }
        clock.value.add(phase, 0);
        clock.value.phase = Some(phase);
        clock.changed = Instant::now();
    }
    /// The most recent reason a provider page was not ready. Labels come from
    /// collector code, so anything that is not a short identifier is dropped.
    pub fn detail(&self, label: &str) {
        let valid = (1..=48).contains(&label.len())
            && label.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
        self.0.lock().expect("job metrics").value.detail = valid.then(|| label.to_owned());
    }
    pub fn counts(&self, counts: Counts) {
        let mut clock = self.0.lock().expect("job metrics");
        clock.value.employees = counts.employees;
        clock.value.timecards = counts.timecards;
        clock.value.itineraries = counts.itineraries;
        clock.value.meals = counts.meals;
        clock.value.rows = counts.rows;
    }
    pub fn snapshot(&self) -> Metrics {
        let clock = self.0.lock().expect("job metrics");
        let mut value = clock.value.clone();
        if value.phase.is_some() {
            value.page_reads_mut().active = clock
                .pages
                .iter()
                .map(|(page, started, changed)| {
                    let mut page = page.clone();
                    page.add(changed.elapsed().as_millis() as u64);
                    page.elapsed_ms = started.elapsed().as_millis() as u64;
                    page
                })
                .collect();
        }
        if let Some(phase) = value.phase {
            value.add(phase, clock.changed.elapsed().as_millis() as u64);
            value.elapsed_ms = clock.started.elapsed().as_millis() as u64;
        }
        value
    }
    pub fn finish(&self, outcome: &str, error: Option<&str>) {
        let mut clock = self.0.lock().expect("job metrics");
        if let Some(phase) = clock.value.phase.take() {
            let elapsed = clock.changed.elapsed().as_millis() as u64;
            clock.value.add(phase, elapsed);
            clock.value.elapsed_ms = clock.started.elapsed().as_millis() as u64;
            clock.value.page_reads_mut().active = clock
                .pages
                .drain(..)
                .map(|(mut page, started, changed)| {
                    page.add(changed.elapsed().as_millis() as u64);
                    page.elapsed_ms = started.elapsed().as_millis() as u64;
                    page
                })
                .collect();
            clock.value.finished_at = Some(db::iso());
            clock.value.outcome = JobOutcome::parse(outcome).expect("known job outcome");
            clock.value.error = error.map(str::to_owned);
            if error.is_none() {
                clock.value.detail = None;
            }
        }
    }
    pub fn observe(&self, sample: Memory) {
        let mut clock = self.0.lock().expect("job metrics");
        let value = &mut clock.value;
        value.memory_samples += 1;
        value.peak_rss_bytes = Some(value.peak_rss_bytes.unwrap_or(0).max(sample.rss));
        if sample.complete {
            value.peak_pss_bytes = Some(value.peak_pss_bytes.unwrap_or(0).max(sample.pss));
            value.peak_private_bytes =
                Some(value.peak_private_bytes.unwrap_or(0).max(sample.private));
        } else {
            value.incomplete_memory_samples += 1;
        }
    }
}
impl Store {
    pub fn save_metrics(&self, job: &str, owner: &str, metrics: &Metrics) -> Result<()> {
        // An interrupted attempt is sealed by recovery. Late writes cannot replace
        // its diagnostics or those of a newer attempt, even after cancellation.
        self.jobs.exec(
            "UPDATE job_metrics SET metrics=? WHERE job_id=? AND attempt=? AND \
            owner=? AND json_extract(metrics,'$.outcome')='running'",
            rusqlite::params![serde_json::to_string(metrics)?, job, metrics.attempt, owner],
        )?;
        Ok(())
    }
    pub fn metrics(&self, job: &str) -> Result<Vec<Metrics>> {
        self.jobs
            .all(
                "SELECT metrics FROM job_metrics WHERE job_id=? ORDER BY attempt",
                [job],
            )?
            .iter()
            .map(|row| Ok(serde_json::from_str(s(row, "metrics"))?))
            .collect()
    }
}

#[derive(Default)]
pub struct Memory {
    pub rss: u64,
    pub pss: u64,
    pub private: u64,
    pub complete: bool,
}
fn field(text: &str, name: &str) -> u64 {
    text.lines()
        .find_map(|line| line.strip_prefix(name))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}
/// Host diagnostics for one supervisor and its descendants. Call on a blocking
/// thread; no browser commands, process arguments or provider data are read.
pub fn memory(root: u32) -> Option<Memory> {
    let mut processes = Vec::new();
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        processes.push((
            pid,
            field(&status, "PPid:") as u32,
            field(&status, "VmRSS:") * 1024,
        ));
    }
    if !processes.iter().any(|(pid, _, _)| *pid == root) {
        return None;
    }
    let mut ids = HashSet::from([root]);
    loop {
        let before = ids.len();
        for &(pid, parent, _) in &processes {
            if ids.contains(&parent) {
                ids.insert(pid);
            }
        }
        if ids.len() == before {
            break;
        }
    }
    let mut result = Memory {
        complete: true,
        ..Default::default()
    };
    for (pid, _, rss) in processes.iter().filter(|(pid, _, _)| ids.contains(pid)) {
        result.rss += rss;
        match fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) {
            Ok(smaps) => {
                if !smaps.lines().any(|line| line.starts_with("Pss:")) {
                    result.complete = false;
                }
                result.pss += field(&smaps, "Pss:") * 1024;
                result.private += (field(&smaps, "Private_Clean:")
                    + field(&smaps, "Private_Dirty:")
                    + field(&smaps, "Private_Hugetlb:"))
                    * 1024;
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(libc::ESRCH) => {}
            Err(_) => result.complete = false,
        }
    }
    Some(result)
}

#[cfg(test)]
#[path = "../tests/backend/metrics.rs"]
mod tests;
