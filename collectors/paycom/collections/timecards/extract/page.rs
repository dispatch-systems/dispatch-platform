//! The timecard page, read into its record: what was observed of it, and each row, punch and
//! change as the page lays them out.
use super::*;

/// The direction of a pending or decided punch change, as `normalizeRequestDetail`.
#[derive(Default)]
pub(super) struct Observed {
    pub(super) operation: Vec<String>,
    pub(super) current_kind: Vec<String>,
    pub(super) current_time: Vec<String>,
    pub(super) requested_kind: Vec<String>,
    pub(super) requested_time: Vec<String>,
    pub(super) note: Vec<String>,
}
impl Observed {
    pub(super) fn field(&mut self, key: usize) -> &mut Vec<String> {
        match key {
            0 => &mut self.operation,
            1 => &mut self.current_kind,
            2 => &mut self.current_time,
            3 => &mut self.requested_kind,
            4 => &mut self.requested_time,
            _ => &mut self.note,
        }
    }
    pub(super) fn normalize(self) -> Read<Change> {
        let fields = [
            &self.operation,
            &self.current_kind,
            &self.current_time,
            &self.requested_kind,
            &self.requested_time,
            &self.note,
        ];
        if fields.iter().all(|values| values.is_empty()) {
            return Ok(Change::empty("unavailable"));
        }
        if fields[..5].iter().any(|values| values.len() != 1) || self.note.len() > 1 {
            return invalid("timecard_provenance_invalid");
        }
        if fields[..5].iter().any(|values| units(&values[0]) > 200) {
            return invalid("timecard_provenance_invalid");
        }
        let operation = match self.operation[0].to_lowercase().as_str() {
            "add" => "add",
            "edit" => "edit",
            "delete" => "delete",
            "type_change" | "type change" => "type_change",
            _ => return invalid("timecard_provenance_invalid"),
        };
        let kind = |value: &str| -> Read<Option<&'static str>> {
            if value.is_empty() {
                return Ok(None);
            }
            match value.to_uppercase().as_str() {
                "IN DAY" => Ok(Some("IN DAY")),
                "OUT LUNCH" | "OUT BREAK" => Ok(Some("OUT LUNCH")),
                "IN LUNCH" | "IN BREAK" => Ok(Some("IN LUNCH")),
                "OUT DAY" => Ok(Some("OUT DAY")),
                _ => invalid("timecard_provenance_invalid"),
            }
        };
        let at = |value: &str| -> Read<Option<String>> {
            if value.is_empty() {
                return Ok(None);
            }
            let upper = value.to_uppercase();
            if time(&upper) {
                Ok(Some(upper))
            } else {
                invalid("timecard_provenance_invalid")
            }
        };
        let current_kind = kind(&self.current_kind[0])?;
        let requested_kind = kind(&self.requested_kind[0])?;
        let current_time = at(&self.current_time[0])?;
        let requested_time = at(&self.requested_time[0])?;
        let note = self.note.first().cloned();
        if note.as_ref().is_some_and(|note| {
            units(note) > 2000 || note.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
        }) {
            return invalid("timecard_provenance_invalid");
        }
        let current = current_kind.is_some() && current_time.is_some();
        let requested = requested_kind.is_some() && requested_time.is_some();
        let complete = match operation {
            "add" => current_kind.is_none() && current_time.is_none() && requested,
            "delete" => current && requested_kind.is_none() && requested_time.is_none(),
            "edit" => {
                current && current_kind == requested_kind && current_time.is_some() && requested
            }
            _ => current && requested && current_kind != requested_kind,
        };
        if !complete {
            return invalid("timecard_provenance_invalid");
        }
        Ok(Change {
            change_operation: Some(operation),
            current_kind,
            current_time,
            requested_kind,
            requested_time,
            change_note: note,
            change_detail_state: "complete",
        })
    }
}

pub(super) struct Page<'a> {
    pub(super) document: &'a Html,
    pub(super) quirks: bool,
}
impl<'a> Page<'a> {
    /// Class and id selectors ignore case only in a quirks-mode document.
    pub(super) fn same(&self, value: &str, expected: &str) -> bool {
        if self.quirks {
            value.eq_ignore_ascii_case(expected)
        } else {
            value == expected
        }
    }
    pub(super) fn has_class(&self, element: ElementRef<'_>, name: &str) -> bool {
        attr(element, "class")
            .is_some_and(|v| v.split_ascii_whitespace().any(|c| self.same(c, name)))
    }
    pub(super) fn has_id(&self, element: ElementRef<'_>, id: &str) -> bool {
        attr(element, "id").is_some_and(|v| self.same(v, id))
    }
    pub(super) fn all(&self) -> impl Iterator<Item = ElementRef<'a>> {
        elements(self.document.tree.root())
    }
    pub(super) fn by_id(&self, id: &str) -> Option<ElementRef<'a>> {
        self.all().find(|e| self.has_id(*e, id))
    }
    /// `closest('.readOnly-combined-cell,[hidden]')`: the element or an ancestor.
    pub(super) fn hidden(&self, element: ElementRef<'_>) -> bool {
        std::iter::once(*element)
            .chain(element.ancestors())
            .filter_map(ElementRef::wrap)
            .any(|e| self.has_class(e, "readOnly-combined-cell") || attr(e, "hidden").is_some())
    }
    /// The time a punch cell shows: Paycom marks it with a class, beside a hidden copy.
    pub(super) fn punch_time(&self, element: Option<ElementRef<'_>>) -> Read<String> {
        let Some(element) = element else {
            return Ok(String::new());
        };
        let values = element
            .child_elements()
            .filter(|child| {
                is(*child, "span")
                    && self.has_class(*child, "current-timecard-cell")
                    && !self.hidden(*child)
            })
            .filter_map(|child| shown_time(&clean(&text(*child, true))))
            .collect::<Vec<_>>();
        match <[String; 1]>::try_from(values) {
            Ok([value]) => Ok(value),
            Err(_) => self.control_value(Some(element)),
        }
    }
    pub(super) fn control_value(&self, element: Option<ElementRef<'_>>) -> Read<String> {
        let Some(element) = element else {
            return Ok(String::new());
        };
        if let Some(select) = elements(*element).find(|e| is(*e, "select")) {
            if select.value().name.ns != ns!(html) {
                return Ok(String::new());
            }
            let value = select_value(select);
            return Ok(clean(&if value.is_empty() {
                selected(select).map(|o| text(*o, true)).unwrap_or_default()
            } else {
                value
            }));
        }
        if let Some(input) = elements(*element).find(|e| is(*e, "input") || is(*e, "textarea")) {
            if input.value().name.ns != ns!(html) {
                return Ok(String::new());
            }
            return Ok(clean(&if is(input, "input") {
                input_value(input)?
            } else {
                textarea_value(input)
            }));
        }
        let triggers = elements(*element)
            .filter(|e| {
                is(*e, "a")
                    && self.has_class(*e, "popoverTrigger")
                    && self.has_class(*e, "popoverTrigger--text")
                    && !self.hidden(*e)
            })
            .collect::<Vec<_>>();
        Ok(match triggers.as_slice() {
            [trigger] => clean(&text(**trigger, true)),
            _ => visible(Some(element)),
        })
    }
    pub(super) fn change(
        &self,
        nodes: &[ElementRef<'_>],
        status: Option<&'static str>,
    ) -> Read<Change> {
        if status.is_none() {
            return Ok(Change::empty("not_applicable"));
        }
        const ATTRIBUTES: [&[&str]; 6] = [
            &["data-pcr-operation", "data-change-operation"],
            &["data-pcr-current-kind", "data-pcr-current-type"],
            &["data-pcr-current-time"],
            &["data-pcr-requested-kind", "data-pcr-requested-type"],
            &["data-pcr-requested-time"],
            &["data-pcr-note", "data-change-note"],
        ];
        const LABELS: [&[&str]; 6] = [
            &["Operation", "Request Operation"],
            &["Current Kind", "Current Type"],
            &["Current Time"],
            &["Requested Kind", "Requested Type"],
            &["Requested Time"],
            &["Request Note", "Change Note"],
        ];
        let mut observed = Observed::default();
        for node in nodes {
            for (key, aliases) in ATTRIBUTES.iter().enumerate() {
                for alias in *aliases {
                    if let Some(value) = attr(*node, alias) {
                        observed.field(key).push(clean(value));
                    }
                }
            }
            for name in ["title", "data-content", "data-original-title"] {
                let Some(raw) = attr(*node, name) else {
                    continue;
                };
                if units(raw) > 4000 {
                    return invalid("timecard_provenance_invalid");
                }
                let content = decode(raw);
                if units(&content) > 4000 {
                    return invalid("timecard_provenance_invalid");
                }
                for line in content.split('\n').map(clean).filter(|l| !l.is_empty()) {
                    for (key, aliases) in LABELS.iter().enumerate() {
                        for alias in *aliases {
                            if let Some(value) = line
                                .strip_prefix(alias)
                                .and_then(|rest| rest.strip_prefix(':'))
                            {
                                observed.field(key).push(clean(value));
                            }
                        }
                    }
                }
            }
        }
        observed.normalize()
    }
    pub(super) fn punch(
        &self,
        element: Option<ElementRef<'_>>,
        slot: &'static str,
        row_index: usize,
    ) -> Read<Option<Punch>> {
        let display_time = self.punch_time(element)?;
        let Some(element) = element.filter(|_| !display_time.is_empty() && display_time != "??")
        else {
            return Ok(None);
        };
        let nodes = std::iter::once(element)
            .chain(elements(*element))
            .collect::<Vec<_>>();
        if nodes.len() > 128 {
            return invalid("timecard_pcr_marker_invalid");
        }
        let mut markers = Vec::new();
        for node in &nodes {
            let mut seen = HashSet::new();
            for token in attr(*node, "class").unwrap_or("").split_ascii_whitespace() {
                if !seen.insert(token) {
                    continue;
                }
                if token
                    .get(..3)
                    .is_some_and(|p| p.eq_ignore_ascii_case("pcr"))
                {
                    if units(token) > 100 {
                        return invalid("timecard_pcr_marker_invalid");
                    }
                    markers.push(token);
                }
                if markers.len() > 256 {
                    return invalid("timecard_pcr_marker_invalid");
                }
            }
        }
        let status = match markers.as_slice() {
            [] => None,
            ["pcrApproved"] => Some("approved"),
            ["pcrPending"] => Some("pending"),
            ["pcrRejected"] => Some("rejected"),
            _ => return invalid("timecard_pcr_marker_invalid"),
        };
        let change = self.change(&nodes, status)?;
        let provenance = elements(*element)
            .find(|e| attr(*e, "title").is_some_and(|t| t.contains("Actual:")))
            .and_then(|e| attr(e, "title"))
            .filter(|raw| !raw.is_empty());
        let mut punch = Punch {
            ordinal: 0,
            row_index,
            slot,
            kind: String::new(),
            display_time,
            actual_time: String::new(),
            rounded_time: String::new(),
            clock_name: String::new(),
            clock_code: String::new(),
            comment: String::new(),
            provenance_available: false,
            change_request_status: status,
            approved: status == Some("approved"),
            change,
        };
        let Some(raw) = provenance else {
            return Ok(Some(punch));
        };
        let decoded = decode(raw);
        let lines = decoded
            .split('\n')
            .map(clean)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>();
        let field = |name: &str| {
            let prefix = format!("{}:", name.to_lowercase());
            lines
                .iter()
                .find(|line| line.to_lowercase().starts_with(&prefix))
                .map(|line| clean(slice_from(line, units(name) + 1)))
                .unwrap_or_default()
        };
        punch.kind = punch_kind(lines.first().map_or("", String::as_str));
        (punch.clock_name, punch.clock_code) = clock(&field("Clock"));
        punch.actual_time = field("Actual");
        punch.rounded_time = field("Rounded");
        punch.comment = field("Comment");
        punch.provenance_available = true;
        Ok(Some(punch))
    }
    pub(super) fn row(
        &self,
        row: ElementRef<'_>,
        headers: &[String],
        row_index: usize,
    ) -> Read<Row> {
        let cells = row.child_elements().collect::<Vec<_>>();
        let cell = |name: &str| {
            headers
                .iter()
                .position(|h| h == name)
                .and_then(|i| cells.get(i).copied())
        };
        let mut unresolved = Vec::new();
        let mut punches = Vec::new();
        for slot in SLOTS {
            let element = cell(slot);
            if element.is_some_and(|e| clean(&text(*e, true)) == "??") {
                unresolved.push(slot);
            }
            if let Some(punch) = self.punch(element, slot, row_index)? {
                punches.push(punch);
            }
        }
        let waiver = cell("waiver").and_then(|c| {
            elements(*c).find(|e| {
                is(*e, "input")
                    && attr(*e, "type").is_some_and(|t| t.eq_ignore_ascii_case("checkbox"))
            })
        });
        let comments = cell("comment")
            .map(|c| {
                elements(*c)
                    .filter_map(|e| attr(e, "title"))
                    .map(|title| trim(comment(&decode(title))).to_owned())
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Row {
            pay_code: self.control_value(cell("paycode"))?,
            allocation1: visible(cell("allocation1")),
            allocation2: visible(cell("allocation2")),
            hours: numeric(&visible(cell("hours"))),
            total_hours: numeric(&visible(cell("total_hours"))),
            dollars: numeric(&visible(cell("amount"))),
            exception_text: visible(cell("exception-points")),
            waiver_checked: waiver
                .map(|w| w.value().name.ns == ns!(html) && attr(w, "checked").is_some()),
            comments,
            unresolved_slots: unresolved,
            punches,
        })
    }
    /// `tbody` children of `table`, and their `tr` children.
    pub(super) fn rows(table: ElementRef<'a>) -> impl Iterator<Item = ElementRef<'a>> {
        table
            .child_elements()
            .filter(|e| is(*e, "tbody"))
            .flat_map(|body| body.child_elements().filter(|e| is(*e, "tr")))
    }
    pub(super) fn generic(&self, id: &str) -> Vec<Vec<String>> {
        let Some(found) = self.by_id(id) else {
            return Vec::new();
        };
        Self::rows(found)
            .map(|row| {
                row.child_elements()
                    .map(|c| visible(Some(c)))
                    .collect::<Vec<_>>()
            })
            .filter(|row| {
                row.iter().any(|v| !v.is_empty())
                    && !row.join(" ").eq_ignore_ascii_case("No Records Found")
            })
            .collect()
    }
    pub(super) fn employee(&self) -> Read<String> {
        let mut values = Vec::new();
        for element in self.all() {
            let named = is(element, "input")
                && (attr(element, "name") == Some("firstrefno")
                    || self.has_id(element, "firstrefno"));
            let first = attr(element, "data-firstrefno");
            let code = attr(element, "data-employee-code");
            if !(named || first.is_some() || code.is_some()) {
                continue;
            }
            let value = value_property(element)?;
            let chosen = [value.as_deref(), first, code]
                .into_iter()
                .flatten()
                .find(|v| !v.is_empty())
                .unwrap_or("");
            let value = clean(chosen);
            if !value.is_empty() {
                values.push(value);
            }
        }
        if values.is_empty()
            || values
                .iter()
                .any(|v| v.len() != 4 || !v.bytes().all(|b| b.is_ascii_alphanumeric()))
        {
            return Ok(String::new());
        }
        let codes = values
            .iter()
            .map(|v| v.to_ascii_uppercase())
            .collect::<HashSet<_>>();
        Ok(if codes.len() == 1 {
            codes.into_iter().next().unwrap_or_default()
        } else {
            String::new()
        })
    }
    /// `document.title`: the first `<title>`'s own text, whitespace collapsed.
    pub(super) fn title(&self) -> String {
        self.all()
            .find(|e| html(*e, "title"))
            .map(|title| {
                textarea_value(title)
                    .split_ascii_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    }
    pub(super) fn record(&self, source: &Source) -> Read<Record> {
        let table = self
            .by_id("tbltimesheet")
            .ok_or(Unreadable::Invalid("timecard_identity_invalid"))?;
        let headers = elements(*table)
            .filter(|e| {
                attr(*e, "data-column").is_some()
                    && e.ancestors()
                        .filter_map(ElementRef::wrap)
                        .any(|a| is(a, "thead"))
            })
            .map(|e| attr(e, "data-column").unwrap_or("").to_owned())
            .collect::<Vec<_>>();
        let dates = source.period["dates"].as_array();
        let rows = Self::rows(table).collect::<Vec<_>>();
        let first = |row: &ElementRef<'_>| {
            row.child_elements()
                .next()
                .map(|cell| text(*cell, true))
                .unwrap_or_default()
        };
        let mut days = Vec::new();
        let mut additional = Vec::new();
        let mut next = [1usize; 14];
        for row in &rows {
            let heading = first(row);
            if let Some((label, date_text)) = day_header(&heading) {
                let projected = self.row(*row, &headers, 0)?;
                let expected = dates
                    .and_then(|d| d.get(days.len()))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let mut punches = projected.punches;
                for (index, punch) in punches.iter_mut().enumerate() {
                    punch.ordinal = index + 1;
                }
                days.push(Day {
                    date: if expected.get(5..).map(|md| md.replacen('-', "/", 1)) == Some(date_text)
                    {
                        expected.to_owned()
                    } else {
                        String::new()
                    },
                    label,
                    pay_code: projected.pay_code,
                    allocation1: projected.allocation1,
                    allocation2: projected.allocation2,
                    hours: projected.hours,
                    total_hours: projected.total_hours,
                    dollars: projected.dollars,
                    exception_text: projected.exception_text,
                    waiver_checked: projected.waiver_checked,
                    comments: projected.comments,
                    missing_punch: !projected.unresolved_slots.is_empty(),
                    unresolved_slots: projected
                        .unresolved_slots
                        .iter()
                        .map(|s| (*s).to_owned())
                        .collect(),
                    punches,
                });
                continue;
            }
            if weekly(*row)
                || days.is_empty()
                || row.child_elements().count() != headers.len()
                || !clean(&heading).is_empty()
            {
                continue;
            }
            let day_index = days.len() - 1;
            let Some(slot) = next.get_mut(day_index) else {
                return invalid("timecard_day_count_invalid");
            };
            let row_index = *slot;
            *slot += 1;
            let projected = self.row(*row, &headers, row_index)?;
            let day = &mut days[day_index];
            let start = day.punches.len();
            let mut ordinals = Vec::new();
            for (index, mut punch) in projected.punches.into_iter().enumerate() {
                punch.ordinal = start + index + 1;
                ordinals.push(punch.ordinal);
                day.punches.push(punch);
            }
            day.unresolved_slots.extend(
                projected
                    .unresolved_slots
                    .iter()
                    .map(|slot| format!("{row_index}:{slot}")),
            );
            day.missing_punch = !day.unresolved_slots.is_empty();
            additional.push(AdditionalRow {
                date: day.date.clone(),
                row_index,
                row_class: clean(attr(*row, "class").unwrap_or("")),
                pay_code: projected.pay_code,
                allocation1: projected.allocation1,
                allocation2: projected.allocation2,
                hours: projected.hours,
                total_hours: projected.total_hours,
                dollars: projected.dollars,
                exception_text: projected.exception_text,
                waiver_checked: projected.waiver_checked,
                comments: projected.comments,
                unresolved_slots: projected.unresolved_slots,
                punch_ordinals: ordinals,
            });
        }
        let weekly_totals = rows
            .iter()
            .filter(|row| weekly(**row))
            .map(|row| {
                numeric(
                    &row.child_elements()
                        .nth(1)
                        .map(|cell| text(*cell, true))
                        .unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        let period_total_hours = weekly_totals
            .iter()
            .try_fold(0., |sum, value| value.map(|v| sum + v))
            .map(fixed);
        let bound = |key: &str| source.period[key].as_str().unwrap_or("").to_owned();
        Ok(Record {
            version: 2,
            source_format: "paycom-timecard-dom.v2",
            employee_code: self.employee()?,
            period_start: bound("start"),
            period_end: bound("end"),
            period_key: bound("key"),
            source_url: source.url.to_owned(),
            page_title: self.title(),
            headers,
            days,
            additional_rows: additional,
            weekly_totals,
            period_total_hours,
            approvals: self.generic("approvals-table"),
            attestations: self.generic("timecard-attestation-table"),
            meal_waivers: self.generic("meal-waivers-table"),
        })
    }
}
