//! Response caps and byte-budget fitting, keeping the newest items of each list.
use super::answer::Found;
use serde::Serialize;

/// The room an answer's JSON fits in, below the server's 48 KB, with its words beside it.
const ROOM: usize = 40_000;
/// The most each long list keeps, the newest, before any is cut further to fit.
const CAPS: [(List, usize); 5] = [
    (List::Packages, 40),
    (List::Returned, 30),
    (List::Events, 30),
    (List::Undelivered, 30),
    (List::Inspections, 30),
];

/// A list an answer may cut short.
#[derive(Clone, Copy)]
enum List {
    Packages,
    DailyDays,
    Returned,
    Events,
    Routes,
    Undelivered,
    TimecardDays,
    Inspections,
}
impl List {
    const ALL: [List; 8] = [
        List::Packages,
        List::DailyDays,
        List::Returned,
        List::Events,
        List::Routes,
        List::Undelivered,
        List::TimecardDays,
        List::Inspections,
    ];
    /// How many it holds in `found`, and their size as JSON.
    fn size(self, found: &mut Found) -> (usize, usize) {
        self.of(found).map_or((0, 0), |list| list.size())
    }
    /// Keeps its newest `most` in `found`, answering how many it cut.
    fn keep(self, found: &mut Found, most: usize) -> usize {
        self.of(found).map_or(0, |mut list| list.keep(most))
    }
    /// The list in `found`, where its section is.
    fn of(self, found: &mut Found) -> Option<Box<dyn Trim + '_>> {
        Some(match self {
            List::Packages => {
                let s = found.scorecards.as_mut()?;
                cut([&mut s.packages], true, &mut s.more_packages)
            }
            List::DailyDays => {
                let d = found.daily.as_mut()?;
                cut([&mut d.days], true, &mut d.more_days)
            }
            List::Returned => {
                let d = found.daily.as_mut()?;
                let newest = d.days.iter_mut().rev().map(|day| &mut day.returned);
                cut(newest, false, &mut d.more_returned)
            }
            List::Events => {
                let d = found.daily.as_mut()?;
                let newest = d.days.iter_mut().rev().map(|day| &mut day.events);
                cut(newest, false, &mut d.more_events)
            }
            List::Routes => {
                let r = found.routes.as_mut()?;
                cut([&mut r.routes], true, &mut r.more_routes)
            }
            List::Undelivered => {
                let r = found.routes.as_mut()?;
                let newest = r.routes.iter_mut().rev().map(|it| &mut it.undelivered);
                cut(newest, false, &mut r.more_undelivered)
            }
            List::TimecardDays => {
                let t = found.timecards.as_mut()?;
                cut([&mut t.days], true, &mut t.more_days)
            }
            List::Inspections => {
                let i = found.inspections.as_mut()?;
                cut([&mut i.inspections], false, &mut i.more_inspections)
            }
        })
    }
}

/// A list of any kind, to measure and cut short.
trait Trim {
    /// How many it holds, and their size as JSON.
    fn size(&self) -> (usize, usize);
    /// Keeps its newest `most`, counting the rest, and answers how many it cut.
    fn keep(&mut self, most: usize) -> usize;
}
/// A list as its parts: one listed oldest first, cut `from_front`; or parts listed newest first,
/// each cut from its end. `more` counts what it leaves out.
struct Cut<'a, T> {
    parts: Vec<&'a mut Vec<T>>,
    from_front: bool,
    more: &'a mut usize,
}
fn cut<'a, T: Serialize + 'a>(
    parts: impl IntoIterator<Item = &'a mut Vec<T>>,
    from_front: bool,
    more: &'a mut usize,
) -> Box<dyn Trim + 'a> {
    let parts = parts.into_iter().collect();
    Box::new(Cut {
        parts,
        from_front,
        more,
    })
}
impl<T: Serialize> Trim for Cut<'_, T> {
    fn size(&self) -> (usize, usize) {
        let count = self.parts.iter().map(|part| part.len()).sum();
        (count, self.parts.iter().map(json_len).sum())
    }
    fn keep(&mut self, most: usize) -> usize {
        let (mut left, mut cut) = (most, 0);
        for part in &mut self.parts {
            let kept = part.len().min(left);
            let gone = part.len() - kept;
            if self.from_front {
                part.drain(..gone);
            } else {
                part.truncate(kept);
            }
            (left, cut) = (left - kept, cut + gone);
        }
        *self.more += cut;
        cut
    }
}

/// Cuts each long list to its cap; then, while the answer takes more than `ROOM`, halves the list
/// taking the most of it. Any cut notes that the answer was trimmed.
pub(super) fn fit(found: &mut Found) {
    for (list, most) in CAPS {
        let cut = list.keep(found, most);
        found.trimmed |= cut > 0;
    }
    while json_len(found) > ROOM {
        let sizes = List::ALL.map(|list| (list, list.size(found)));
        let held = sizes.into_iter().filter(|(_, (count, _))| *count > 0);
        let Some((list, (count, _))) = held.max_by_key(|(_, (_, bytes))| *bytes) else {
            break;
        };
        list.keep(found, count / 2);
        found.trimmed = true;
    }
}
fn json_len<T: Serialize + ?Sized>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(0, |json| json.len())
}

#[cfg(test)]
#[path = "../../tests/backend/tools/driver.rs"]
mod tests;
