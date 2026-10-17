//! Proving that a job's responses, read over HTTP, equal its rendered pages, before the job
//! relies on them: which employees it renders to prove it, and whether reading direct is on.
use super::*;

/// Whether this job's responses, read here, equal its rendered pages: at least one
/// comparison covered punches and none differed.
#[derive(Default)]
pub(super) struct Proof {
    pub(super) differed: AtomicBool,
    pub(super) punched: AtomicBool,
}
impl Proof {
    pub(super) fn proven(&self) -> bool {
        self.punched.load(Ordering::SeqCst) && !self.differed.load(Ordering::SeqCst)
    }
    /// The employees a job renders to prove its responses: the first the roster
    /// shows with hours, so the comparison covers punches, and a few at random.
    pub(super) fn sample(raw: &Value, employees: &[Value], todo: &[usize]) -> Vec<usize> {
        let worked = raw["employees"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| {
                row["totals"]["totalHours"]
                    .as_f64()
                    .is_some_and(|hours| hours > 0.)
            })
            .map(|row| s(row, "employeeCode").to_ascii_uppercase())
            .collect::<HashSet<_>>();
        let first = todo
            .iter()
            .copied()
            .find(|index| worked.contains(&s(&employees[*index], "code").to_ascii_uppercase()))
            .unwrap_or(todo[0]);
        let rest = todo
            .iter()
            .copied()
            .filter(|index| *index != first)
            .collect::<Vec<_>>();
        std::iter::once(first)
            .chain(pick(&rest, Direct::SAMPLE))
            .collect()
    }
}

/// Up to `count` distinct values of `from`, chosen at random. Without entropy the
/// first ones are chosen instead.
pub(super) fn pick(from: &[usize], count: usize) -> Vec<usize> {
    let mut chosen = Vec::new();
    if from.is_empty() {
        return chosen;
    }
    let mut random = [0u8; 8 * Direct::SAMPLE * 4];
    let drawn = getrandom::fill(&mut random).is_ok();
    for (index, bytes) in random.chunks_exact(8).enumerate() {
        if chosen.len() == count.min(from.len()) {
            break;
        }
        let position = if drawn {
            u64::from_le_bytes(bytes.try_into().expect("eight bytes")) as usize
        } else {
            index
        };
        let value = from[position % from.len()];
        if !chosen.contains(&value) {
            chosen.push(value);
        }
    }
    chosen
}

/// Reads one employee both ways and keeps the rendered read. A punch can land
/// between reads, so only a response that still differs after it counts.
#[allow(clippy::too_many_arguments)]
pub(super) async fn prove(
    page: &mut Page,
    http: &Http,
    proof: &Proof,
    origin: &str,
    employee: &Value,
    period: &Value,
    metrics: &Recorder,
    ordinal: usize,
) -> Result<Vec<Value>> {
    let response = read_http(http, origin, employee, period, metrics, None).await;
    let rendered = read_rendered(page, origin, employee, period, metrics, ordinal, false).await?;
    let agrees = response.is_ok_and(|records| records == rendered)
        || read_http(http, origin, employee, period, metrics, None)
            .await
            .is_ok_and(|records| records == rendered);
    if agrees {
        metrics.spot_checked();
        if rendered
            .iter()
            .any(|card| card["punches"].as_array().is_some_and(|p| !p.is_empty()))
        {
            proof.punched.store(true, Ordering::SeqCst);
        }
    } else {
        proof.differed.store(true, Ordering::SeqCst);
    }
    Ok(rendered)
}

/// Whether this job may read timecards from responses. A page can fill or change
/// its table after loading, which a response would miss while still validating, so
/// each job first requires a response to equal a rendered read that has punches.
/// One disagreement keeps the whole job on rendered reads.
///
/// That proves one employee. A few more, chosen at random in each job, are also
/// rendered after their response is read, so a difference limited to some
/// employees cannot be published for long without failing a collection.
pub(super) struct Direct {
    pub(super) state: AtomicU8,
    pub(super) sample: HashSet<usize>,
}
impl Direct {
    pub(super) const ENABLED: u8 = 1;
    pub(super) const DISABLED: u8 = 2;
    pub(super) const VERIFYING: u8 = 3;
    pub(super) const SAMPLE: usize = 4;
    /// A roster this long gains from responses; a shorter one is rendered.
    pub(super) const SAMPLED_ROSTER: usize = 20;
    /// `todo` holds employee indexes in reading order. The first two are rendered
    /// before any response is trusted, and a short roster gains little from
    /// responses, so neither is sampled.
    pub(super) fn new(todo: &[usize]) -> Self {
        let later = todo.get(2..).unwrap_or_default();
        let sample = if todo.len() >= Self::SAMPLED_ROSTER {
            pick(later, Self::SAMPLE)
                .into_iter()
                .map(|index| index + 1)
                .collect()
        } else {
            HashSet::new()
        };
        Self {
            state: AtomicU8::new(0),
            sample,
        }
    }
    /// Every read rendered.
    pub(super) fn disabled() -> Self {
        Self {
            state: AtomicU8::new(Self::DISABLED),
            sample: HashSet::new(),
        }
    }
    pub(super) fn enabled(&self) -> bool {
        self.state.load(Ordering::SeqCst) == Self::ENABLED
    }
    pub(super) async fn verify(
        &self,
        page: &Page,
        origin: &str,
        employee: &Value,
        period: &Value,
        rendered: &[Value],
    ) -> Result<()> {
        let punched = rendered
            .iter()
            .any(|card| card["punches"].as_array().is_some_and(|p| !p.is_empty()));
        // One lane verifies; the other keeps rendering until the result is known.
        if !punched
            || self
                .state
                .compare_exchange(0, Self::VERIFYING, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return Ok(());
        }
        let response = read_response(page, origin, employee, period, true).await;
        self.state.store(
            if response
                .as_ref()
                .is_ok_and(|r| r.as_deref() == Some(rendered))
            {
                Self::ENABLED
            } else {
                Self::DISABLED
            },
            Ordering::SeqCst,
        );
        response.map(|_| ())
    }
}
