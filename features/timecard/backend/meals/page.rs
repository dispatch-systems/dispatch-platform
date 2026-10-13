//! Table selection over one immutable day's assessment. Export and paging use the
//! same search, filter and ordering; source evidence remains available on demand.
use crate::api::{
    assessment::MealStatus,
    meals::{MealComparison, MealComparisonPage, MealCounts, MealEmployee},
};
use dispatch_core::foundation::names::{compare, display_name};

pub(crate) struct Query<'a> {
    pub search: &'a str,
    pub filter: &'a str,
    pub sort: &'a str,
    pub desc: bool,
    pub offset: usize,
    pub limit: Option<usize>,
    pub name_order: &'a str,
}
pub(crate) fn page(
    day: &MealComparison,
    query: Query<'_>,
    ordering: &[usize],
) -> MealComparisonPage {
    let mut counts = MealCounts::default();
    let search = query.search.to_lowercase();
    for row in &day.rows {
        let assessment = &row.assessment;
        counts.all += 1;
        counts.late += usize::from(assessment.late_in);
        counts.different += usize::from(assessment.different);
        counts.missing += usize::from(assessment.missing);
        counts.gaps += usize::from(assessment.long_gap);
    }
    let selected: Vec<_> = ordering
        .iter()
        .copied()
        .filter(|index| {
            let row = &day.rows[*index];
            let assessment = &row.assessment;
            let includes = match query.filter {
                "late" => assessment.late_in,
                "different" => assessment.different,
                "missing" => assessment.missing,
                "gaps" => assessment.long_gap,
                _ => true,
            };
            includes
                && (search.is_empty()
                    || format!(
                        "{} {} {}",
                        person_name(&row.source.name, query.name_order),
                        row.source
                            .paycom
                            .as_ref()
                            .map_or("", |p| p.employee_code.as_str()),
                        row.source
                            .cortex
                            .iter()
                            .map(|m| m.driver_name.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                    .to_lowercase()
                    .contains(&search))
        })
        .collect();
    let total = selected.len();
    let rows = selected
        .into_iter()
        .skip(query.offset)
        .take(query.limit.unwrap_or(usize::MAX))
        .map(|index| day.rows[index].clone())
        .collect();
    MealComparisonPage {
        comparison: MealComparison {
            date: day.date.clone(),
            timezone: day.timezone.clone(),
            rows,
            paycom_collected_at: day.paycom_collected_at.clone(),
            cortex_publications: day.cortex_publications.clone(),
            drivers: day.drivers.clone(),
        },
        total,
        counts,
    }
}
/// Compute keys once per row, then retain only the ordering. Paging and filtering
/// reuse it while the underlying day is unchanged; live collections calculate afresh.
pub(crate) fn order(day: &MealComparison, query: &Query<'_>) -> Vec<usize> {
    let by_name = query.sort == "employee";
    let mut keys: Vec<_> = day
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            (
                index,
                if by_name {
                    person_name(&row.source.name, query.name_order)
                } else {
                    String::new()
                },
                if by_name {
                    None
                } else {
                    numeric(row, query.sort, &day.date)
                },
            )
        })
        .collect();
    keys.sort_by(|(a, a_name, a_number), (b, b_name, b_number)| {
        let order = if by_name {
            compare(a_name, b_name)
        } else {
            a_number.cmp(b_number)
        };
        (if query.desc { order.reverse() } else { order }).then_with(|| a.cmp(b))
    });
    keys.into_iter().map(|(index, _, _)| index).collect()
}
fn person_name(name: &str, order: &str) -> String {
    let full = name.split_once(',').map_or_else(
        || name.to_owned(),
        |(last, first)| format!("{first} {last}"),
    );
    let full = full
        .split(|c: char| (c.is_whitespace() && c != '\u{0085}') || c == '\u{feff}')
        .collect::<Vec<_>>()
        .join(" ");
    if order == "last_first" && name.contains(',') && full.contains(' ') {
        name.into()
    } else {
        display_name(&full, order)
    }
}
// Missing numeric values sort like the table's empty values: first ascending.
fn numeric(row: &MealEmployee, sort: &str, date: &str) -> Option<i32> {
    let a = &row.assessment;
    let pair = a.pairs.first();
    match sort {
        "inDay" => a.paycom.in_day.as_ref().map(|c| c.minute),
        "outDay" => a.paycom.out_day.as_ref().map(|c| c.minute),
        "outLunch" => pair.and_then(|p| p.out_difference).map(i32::abs),
        "inLunch" => pair.and_then(|p| p.in_difference).map(i32::abs),
        "lastDelivery" | "firstDelivery" => {
            let meal = pair
                .and_then(|p| p.cortex_index)
                .and_then(|i| row.source.cortex.get(i))?;
            let instant = if sort == "lastDelivery" {
                &meal.last_delivery
            } else {
                &meal.first_delivery
            };
            super::assessment::cortex_clock(instant.as_deref(), date, &meal.timezone)
                .map(|c| c.minute)
        }
        "comparison" => Some(match a.status {
            MealStatus::CortexUnreadable => 0,
            MealStatus::Different => 1,
            MealStatus::MissingLunch => 2,
            MealStatus::MissingData => 3,
            MealStatus::ReviewPunches => 4,
            MealStatus::ReviewPairing => 5,
            MealStatus::NoFlexMeal => 6,
            MealStatus::FlexOnly => 7,
            MealStatus::Same => 8,
        }),
        _ => None,
    }
}
