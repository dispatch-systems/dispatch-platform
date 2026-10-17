//! One pass over the routes whose record is missing or out of date, shared by the tabs: each
//! route read in a lane, its record kept, and the live view told as it goes.
use super::*;

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
    /// Reads routes through the hook, `in_flight` at once: each is named to the hook and
    /// the application moved to it without waiting for the one before, and what the hook
    /// has read is taken every 50 ms. A route the application didn't answer, or not within
    /// 20 s, is read afterwards from its own loaded page. `loaded` is whether the tab
    /// shows the application already; one that doesn't loads its first route's page.
    pub(super) async fn moves(&self, page: &Page, loaded: bool) -> Result<Vec<Read>> {
        let driver = self.driver;
        let mut reads = Vec::new();
        let mut moving = loaded;
        let mut waiting: Vec<(&'a Candidate, usize, Instant)> = Vec::new();
        let mut unanswered: Vec<(&'a Candidate, usize)> = Vec::new();
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
            let mut drained = false;
            while waiting.len() < self.method.in_flight {
                let Some((candidate, ordinal)) = self.begin().await? else {
                    drained = true;
                    break;
                };
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
                let url = driver.meal_url(self.scope, Some(candidate));
                let moved = named.is_ok_and(|v| v == true)
                    && page
                        .evaluate(&call(MOVE, &json!(url)))
                        .await
                        .is_ok_and(|v| v == true);
                if moved {
                    waiting.push((candidate, ordinal, Instant::now()));
                } else {
                    unanswered.push((candidate, ordinal));
                }
            }
            if waiting.is_empty() {
                if drained {
                    break;
                }
                continue;
            }
            sleep(Duration::from_millis(50)).await;
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
                        let Some(index) = waiting.iter().position(|(c, ..)| c.id == id) else {
                            continue;
                        };
                        let (candidate, ordinal, _) = waiting.swap_remove(index);
                        if value["unanswered"] == true {
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
                if waiting[index].2.elapsed() >= MOVE_DEADLINE {
                    let (candidate, ordinal, _) = waiting.swap_remove(index);
                    unanswered.push((candidate, ordinal));
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
