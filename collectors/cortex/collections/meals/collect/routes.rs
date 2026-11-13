//! One pass over the routes whose record is missing or out of date, shared by the tabs: each
//! route read in a lane, its record kept, and the live view told as it goes.
use super::*;
use crate::connection::gate;
use std::collections::VecDeque;
use tokio::time::sleep_until;

/// One pass over the routes whose record is missing or out of date, shared by the tabs.
pub(super) struct Routes<'a, F> {
    pub(super) driver: &'a Driver,
    pub(super) scope: &'a Scope,
    pub(super) pending: Vec<&'a Candidate>,
    pub(super) next: AtomicUsize,
    pub(super) stopped: AtomicBool,
    pub(super) reads: &'a AtomicUsize,
    pub(super) done: AtomicUsize,
    pub(super) total: usize,
    pub(super) metrics: &'a Recorder,
    pub(super) live: Option<&'a Writer>,
    pub(super) started_at: i64,
    pub(super) progress: &'a F,
    pub(super) method: &'a MealMethod,
}
/// How long a pass waits on Cortex's throttling at most, as a native read does: a longer
/// cooldown ends the collection, whose next attempt finds the account's gate as it left it.
const THROTTLE_BUDGET: Duration = Duration::from_secs(120);
/// A route asked of the application and not yet answered: the try it is on, since when, and
/// its place at the account's gate, given up as it is answered or given up on.
struct Asked<'a> {
    candidate: &'a Candidate,
    ordinal: usize,
    attempt: usize,
    at: Instant,
    _admitted: gate::Admitted,
}
/// A route's record at the revision it was read, or `None` when it changed meanwhile.
pub(super) type Read = (String, Option<(String, Itinerary)>);
impl<'a, F, Fut> Routes<'a, F>
where
    F: Fn(i64, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    pub(super) async fn lane(&self, page: &Page, loaded: bool) -> Result<Vec<Read>> {
        let result = if self.method.hook {
            self.moves(page, loaded).await
        } else {
            self.read(page).await
        };
        if result.is_err() {
            self.stopped.store(true, Ordering::SeqCst);
        }
        result
    }
    /// The next route still to read, its read begun in the metrics, with its ordinal.
    pub(super) async fn begin(&self) -> Result<Option<(&'a Candidate, usize)>> {
        if self.stopped.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let Some(&candidate) = self.pending.get(self.next.fetch_add(1, Ordering::SeqCst)) else {
            return Ok(None);
        };
        let ordinal = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        self.metrics.page_start(ordinal, 1);
        self.metrics.page_stage(ordinal, "content");
        let done = self.done.load(Ordering::SeqCst);
        (self.progress)(
            10 + (70 * done / self.total.max(1)) as i64,
            format!("Reading meal evidence ({}/{})", done + 1, self.total),
        )
        .await?;
        Ok(Some((candidate, ordinal)))
    }
    /// A route's read, kept as the pass keeps it, or the error that ends the collection.
    pub(super) async fn end(
        &self,
        candidate: &Candidate,
        ordinal: usize,
        result: Result<Value>,
    ) -> Result<Read> {
        self.metrics
            .page_finish(ordinal, result.as_ref().err().map(|e| e.code.as_str()));
        match result {
            Ok(value) => {
                let mut route: Itinerary = serde_json::from_value(value["itinerary"].clone())
                    .map_err(|_| Error::new("cortex_content_incomplete", 502))?;
                route.source_url = Some(format!(
                    "{}{}",
                    self.driver.origin,
                    self.scope.detail_path(&candidate.id)
                ));
                ensure(
                    route.id == candidate.id && route.transporter_id == candidate.transporter_id,
                    "cortex_invalid_identity",
                    502,
                )?;
                if let Some(live) = self.live {
                    live.cortex(&Capture {
                        scope: self.scope.clone(),
                        started_at: self.started_at,
                        finished_at: now(),
                        itineraries: vec![route.clone()],
                    })
                    .await?;
                }
                self.done.fetch_add(1, Ordering::SeqCst);
                Ok((
                    candidate.id.clone(),
                    Some((candidate.revision.clone(), route)),
                ))
            }
            // A meal swipe landed after the list was read. The next pass re-reads this
            // route at its new revision.
            Err(error) if error.is(codes::CORTEX_SOURCE_CHANGED) => {
                Ok((candidate.id.clone(), None))
            }
            Err(error) => Err(error),
        }
    }
    /// Reads routes one at a time from their rendered pages.
    pub(super) async fn read(&self, page: &Page) -> Result<Vec<Read>> {
        let mut reads = Vec::new();
        let mut moving = false;
        while let Some((candidate, ordinal)) = self.begin().await? {
            let result = self
                .driver
                .read_route(
                    page,
                    self.scope,
                    candidate,
                    self.method,
                    &mut moving,
                    self.metrics,
                )
                .await;
            reads.push(self.end(candidate, ordinal, result).await?);
            // A rendered page keeps what it showed until collected.
            if reads.len().is_multiple_of(10) {
                page.collect_garbage().await?;
            }
        }
        Ok(reads)
    }
    /// Asks the tab's application for `candidate`'s itinerary: of its own API action
    /// while `asking`, else by moving it to the route. An application whose action
    /// declines stops being asked; one the browser couldn't reach this once is moved for
    /// this route alone. Whether the application took the ask.
    async fn ask(&self, page: &Page, candidate: &Candidate, asking: &mut bool) -> bool {
        let driver = self.driver;
        if *asking {
            let read = json!({
                "endpoint": crate::collections::routes::protocol::itinerary_path(&candidate.id),
                "params": {
                    "documentType": "Itinerary",
                    "itineraryId": candidate.id,
                    "serviceAreaId": self.scope.service_area_id,
                },
            });
            // The store lives in the application's own world, where the action runs.
            match driver.browser.evaluate(&page.id, &call(ASK, &read)).await {
                Ok(taken) if taken == true => return true,
                Ok(_) => {
                    *asking = false;
                    self.metrics.detail("meal_ask_unavailable");
                }
                // An ask the browser timed out on may still have been sent: its answer is
                // read for this route either way, and a second one waits unclaimed.
                Err(_) => self.metrics.detail("meal_ask_failed"),
            }
        }
        let url = driver.meal_url(self.scope, Some(candidate));
        page.evaluate(&call(MOVE, &json!(url)))
            .await
            .is_ok_and(|v| v == true)
    }
    /// Reads routes through the hook, `in_flight` at once and no more than the account's gate
    /// admits (`connection::gate`): each is named to the hook and asked of the application, or
    /// the application moved to it, without waiting for the one before, and what the hook has
    /// read is taken every 50 ms. A route Cortex throttled is asked again the same way once
    /// the account has cooled, fewer at once, never from a loaded page, which would ask Cortex
    /// for more; a pass waits on throttling for `THROTTLE_BUDGET` at most. A route the
    /// application didn't answer otherwise, or not within 20 s, is read afterwards from its
    /// own loaded page. `loaded` is whether the tab shows the application already; one that
    /// doesn't loads its first route's page.
    pub(super) async fn moves(&self, page: &Page, loaded: bool) -> Result<Vec<Read>> {
        let driver = self.driver;
        let gate = gate::of(&driver.origin, &self.scope.provider);
        let mut reads = Vec::new();
        let mut moving = loaded;
        let mut asking = self.method.ask;
        let mut waiting: Vec<Asked<'a>> = Vec::new();
        // Routes Cortex throttled, to ask again, each with its ordinal and the try it is on.
        let mut again: VecDeque<(&'a Candidate, usize, usize)> = VecDeque::new();
        let mut unanswered: Vec<(&'a Candidate, usize)> = Vec::new();
        let mut budget = THROTTLE_BUDGET;
        let mut drained = false;
        loop {
            if !moving {
                let Some((candidate, ordinal)) = self.begin().await? else {
                    break;
                };
                let result = driver
                    .read_route(
                        page,
                        self.scope,
                        candidate,
                        self.method,
                        &mut moving,
                        self.metrics,
                    )
                    .await;
                reads.push(self.end(candidate, ordinal, result).await?);
                continue;
            }
            // A throttle cools the account for every reader of it: once what was asked
            // has drained, nothing is asked before it has passed, and then fewer at once.
            if waiting.is_empty()
                && let Some(until) = gate.cooldown()
            {
                let delay = until.saturating_duration_since(Instant::now());
                ensure(delay <= budget, codes::CORTEX_THROTTLED.as_str(), 502)?;
                budget -= delay;
                sleep_until(until).await;
                gate.cooled(until);
                gate.narrow();
            }
            while gate.cooldown().is_none()
                && waiting.len() < self.method.in_flight.min(gate.width())
            {
                let Some(admitted) = gate.admit_now() else {
                    break;
                };
                let (candidate, ordinal, attempt) = match again.pop_front() {
                    Some(next) => next,
                    None if drained => break,
                    None => match self.begin().await? {
                        Some((candidate, ordinal)) => (candidate, ordinal, 1),
                        None => {
                            drained = true;
                            break;
                        }
                    },
                };
                if attempt > 1 {
                    self.metrics.page_start(ordinal, attempt);
                    self.metrics.page_stage(ordinal, "content");
                }
                let input =
                    json!({"candidate": candidate, "scope": self.scope, "origin": driver.origin});
                let named = driver
                    .browser
                    .evaluate(
                        &page.id,
                        &format!(
                            "!!window.__dispatchMeals && window.__dispatchMeals.want({input})"
                        ),
                    )
                    .await;
                let moved =
                    named.is_ok_and(|v| v == true) && self.ask(page, candidate, &mut asking).await;
                if moved {
                    waiting.push(Asked {
                        candidate,
                        ordinal,
                        attempt,
                        at: Instant::now(),
                        _admitted: admitted,
                    });
                } else {
                    unanswered.push((candidate, ordinal));
                }
            }
            if waiting.is_empty() && again.is_empty() && drained {
                break;
            }
            sleep(Duration::from_millis(50)).await;
            if waiting.is_empty() {
                continue;
            }
            match driver
                .browser
                .evaluate(
                    &page.id,
                    "window.__dispatchMeals ? window.__dispatchMeals.taken() : null",
                )
                .await
            {
                Ok(Value::Object(taken)) => {
                    for (id, value) in taken {
                        let Some(index) = waiting.iter().position(|a| a.candidate.id == id) else {
                            continue;
                        };
                        let asked = waiting.swap_remove(index);
                        let (candidate, ordinal) = (asked.candidate, asked.ordinal);
                        if value["throttled"] == true {
                            gate.throttled(gate::PAUSE);
                            self.metrics.detail("meal_throttled");
                            self.metrics.page_cancel(ordinal);
                            again.push_back((candidate, ordinal, asked.attempt + 1));
                        } else if value["unanswered"] == true {
                            self.metrics.detail("meal_hook_unanswered");
                            unanswered.push((candidate, ordinal));
                        } else {
                            let result = evidence(value, self.metrics);
                            reads.push(self.end(candidate, ordinal, result).await?);
                        }
                    }
                }
                Ok(_) => (),
                Err(error) if error.is_any(CONTENT_NOT_READY) => (),
                Err(error) => return Err(error),
            }
            let mut index = 0;
            while index < waiting.len() {
                if waiting[index].at.elapsed() >= MOVE_DEADLINE {
                    let asked = waiting.swap_remove(index);
                    unanswered.push((asked.candidate, asked.ordinal));
                } else {
                    index += 1;
                }
            }
        }
        // The application didn't answer these: each is read from its own page.
        for (candidate, ordinal) in unanswered {
            if self.stopped.load(Ordering::SeqCst) {
                break;
            }
            self.metrics.detail("meal_move_reloaded");
            let mut reload = false;
            let result = driver
                .read_route(
                    page,
                    self.scope,
                    candidate,
                    self.method,
                    &mut reload,
                    self.metrics,
                )
                .await;
            reads.push(self.end(candidate, ordinal, result).await?);
        }
        Ok(reads)
    }
}
