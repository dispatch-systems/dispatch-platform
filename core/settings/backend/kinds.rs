//! What each kind of field holds, every concern of a kind in one place: the options it
//! offers, how Settings draws it, a value sent checked and made the one way it's kept, what a
//! kept value still fits, and how the log shows one. A new kind is one arm in each.
use super::api::types::{
    SettingsKind, SettingsOption, SettingsPreset, SettingsScheduleChoice, SettingsScheduleOption,
    SettingsStart,
};
use crate::{
    Error, Result,
    collection::api::types::ScheduleCollection,
    db::Store,
    ensure,
    manifest::settings::{Field, Kind, Listed, Options, ScheduleChoice},
};
use serde_json::Value;

/// Its value until one is saved: none for `schedules`, which holds none.
pub(crate) fn default(field: &Field) -> Option<Value> {
    Some(match field.kind {
        Kind::Toggle { default } => Value::from(default),
        Kind::Choice { default, .. } | Kind::Time { default } | Kind::Timezone { default } => {
            Value::from(default)
        }
        Kind::Text { default, .. } => Value::from(default),
        Kind::Pick { default, .. } => default.map_or(Value::Null, Value::from),
        Kind::Many { default, .. } => Value::from(default.to_vec()),
        Kind::Number { default, .. } => default.map_or(Value::Null, Value::from),
        Kind::Schedules { .. } => return None,
    })
}

/// The options fields offer as they're read for one answer, `dsp`'s alone: each list a
/// feature makes read once, however many fields offer it, as a DSP's departments may be by
/// several sections.
pub(crate) struct Lists<'a> {
    db: &'a Store,
    dsp: &'a str,
    read: Vec<(Listed, Vec<(String, String)>)>,
}
impl<'a> Lists<'a> {
    pub(crate) fn new(db: &'a Store, dsp: &'a str) -> Self {
        let read = Vec::new();
        Self { db, dsp, read }
    }
    /// The options `field` offers, each a value and its label: none for a kind without
    /// options. A listed kind's are its feature's to list.
    pub(crate) fn options(&mut self, field: &Field) -> Result<Option<Vec<(String, String)>>> {
        let fixed = |options: &[(&str, &str)]| {
            options
                .iter()
                .map(|(value, label)| ((*value).to_owned(), (*label).to_owned()))
                .collect()
        };
        Ok(match field.kind {
            Kind::Choice { options, .. }
            | Kind::Many {
                options: Options::Fixed(options),
                ..
            } => Some(fixed(options)),
            Kind::Pick { options, .. }
            | Kind::Many {
                options: Options::Listed(options),
                ..
            } => Some(self.listed(options)?),
            _ => None,
        })
    }
    /// What `list` lists, read once. One that fails isn't kept, so the next field that offers
    /// it reads it again.
    fn listed(&mut self, list: Listed) -> Result<Vec<(String, String)>> {
        let read = self
            .read
            .iter()
            .find(|(read, _)| std::ptr::fn_addr_eq(*read, list));
        if let Some((_, options)) = read {
            return Ok(options.clone());
        }
        let options = list(self.db, self.dsp)?;
        self.read.push((list, options.clone()));
        Ok(options)
    }
}

/// What Settings draws `field` with, given the options it offers now.
pub(crate) fn described(
    field: &Field,
    options: Option<Vec<(String, String)>>,
) -> Result<SettingsKind> {
    let options: Vec<SettingsOption> = (options.unwrap_or_default().into_iter())
        .map(|(value, label)| SettingsOption { value, label })
        .collect();
    let words = |text: Option<&str>| text.map(str::to_owned);
    Ok(match field.kind {
        Kind::Toggle { .. } => SettingsKind::Toggle,
        Kind::Choice { .. } => SettingsKind::Choice { options },
        Kind::Pick { none, .. } => SettingsKind::Pick {
            options,
            none: words(none),
        },
        Kind::Many { ordered, .. } => SettingsKind::Many { options, ordered },
        Kind::Number {
            min,
            max,
            unit,
            presets,
            none,
            ..
        } => SettingsKind::Number {
            min,
            max,
            unit: words(unit),
            presets: presets
                .iter()
                .map(|&(value, label)| SettingsPreset {
                    value,
                    label: label.to_owned(),
                })
                .collect(),
            none: words(none),
        },
        Kind::Text {
            min,
            max,
            code,
            locked,
            ..
        } => SettingsKind::Text {
            min,
            max,
            code,
            locked,
        },
        Kind::Time { .. } => SettingsKind::Time,
        Kind::Timezone { .. } => SettingsKind::Timezone,
        Kind::Schedules {
            path,
            collection,
            starts: (name, at),
            collects,
        } => SettingsKind::Schedules {
            path: path.to_owned(),
            collection: schedule_collection(collection)?,
            starts: SettingsStart {
                name: name.to_owned(),
                local_time: at.to_owned(),
            },
            collects: collects.map(schedule_choice).transpose()?,
        },
    })
}
/// What each schedule of a schedules field chooses to collect, as Settings draws it.
fn schedule_choice(choice: ScheduleChoice) -> Result<SettingsScheduleChoice> {
    let options = choice
        .options
        .iter()
        .map(|(collection, label)| {
            Ok(SettingsScheduleOption {
                collection: schedule_collection(collection)?,
                label: (*label).to_owned(),
            })
        })
        .collect::<Result<_>>()?;
    Ok(SettingsScheduleChoice {
        label: choice.label.to_owned(),
        options,
    })
}
/// A collection a schedules field names, which the registry's check proved one a schedule
/// may name.
fn schedule_collection(collection: &str) -> Result<ScheduleCollection> {
    ScheduleCollection::parse(collection)
        .ok_or_else(|| Error::new("schedule_collection_unknown", 500))
}

/// `given` as `field` keeps it, given what it holds now and the options it offers: made the
/// one way it's kept, and refused as `invalid_input` unless it fits, is one of the options
/// listed now or one it holds already, which its list may have dropped since, and neither
/// empties a text once set nor changes a locked one. Given no options,
/// what a field lists as it's read is checked only as `fits` checks it, as for values a
/// feature adopts, which its list may have dropped since they were kept.
pub(crate) fn checked(
    field: &Field,
    given: &Value,
    now: &Value,
    options: Option<&[(String, String)]>,
) -> Result<Value> {
    let listing = options;
    let options = options.unwrap_or_default();
    let offered = |value: &Value| {
        let value = value.as_str();
        options
            .iter()
            .any(|(option, _)| Some(option.as_str()) == value)
    };
    let value = match (field.kind, given) {
        (Kind::Schedules { .. }, _) => return Err(Error::new("invalid_input", 400)),
        (Kind::Text { code, .. }, Value::String(text)) => {
            let text = text.trim();
            Value::from(if code {
                text.to_uppercase()
            } else {
                text.to_owned()
            })
        }
        // Unordered, they're kept in their options' order, so a change is only one of which.
        (Kind::Many { ordered: false, .. }, Value::Array(values)) => {
            let mut values = values.clone();
            values.sort_by_key(|value| {
                options
                    .iter()
                    .position(|(v, _)| Some(v.as_str()) == value.as_str())
            });
            Value::from(values)
        }
        _ => given.clone(),
    };
    // What it holds already stays allowed, so that saving others doesn't force it out.
    let held =
        |value: &Value| *now == *value || now.as_array().is_some_and(|kept| kept.contains(value));
    let listed = match (field.kind, listing) {
        (Kind::Pick { .. }, Some(_)) => value.is_null() || offered(&value) || held(&value),
        (
            Kind::Many {
                options: Options::Listed(_),
                ..
            },
            Some(_),
        ) => value
            .as_array()
            .is_some_and(|values| values.iter().all(|value| offered(value) || held(value))),
        _ => true,
    };
    let set = now.as_str().is_some_and(|now| !now.is_empty());
    let kept = match field.kind {
        Kind::Text { locked, .. } if set => value != "" && !(locked && *now != value),
        _ => true,
    };
    ensure(fits(field, &value) && listed && kept, "invalid_input", 400)?;
    Ok(value)
}

/// Whether `value` fits `field` as it's declared now: its kind, its fixed options and its
/// range. A kept one that doesn't reads as the default, and a default that doesn't is refused
/// as the registry is installed. What a field lists as it's read is checked only as a value is
/// saved.
pub(crate) fn fits(field: &Field, value: &Value) -> bool {
    let fixed = |options: &[(&str, &str)], value: &Value| {
        value
            .as_str()
            .is_some_and(|value| options.iter().any(|(v, _)| *v == value))
    };
    match field.kind {
        Kind::Toggle { .. } => value.is_boolean(),
        Kind::Choice { options, .. } => fixed(options, value),
        Kind::Pick { none, .. } => value.is_string() || (value.is_null() && none.is_some()),
        Kind::Many { options, .. } => value.as_array().is_some_and(|values| {
            let once = (values.iter().enumerate()).all(|(at, value)| !values[..at].contains(value));
            once && match options {
                Options::Fixed(options) => values.iter().all(|value| fixed(options, value)),
                Options::Listed(_) => values.iter().all(Value::is_string),
            }
        }),
        Kind::Number { min, max, none, .. } => match value.as_i64() {
            Some(number) => (min..=max).contains(&number),
            None => value.is_null() && none.is_some(),
        },
        Kind::Text {
            min,
            max,
            default,
            code,
            ..
        } => value.as_str().is_some_and(|text| {
            let length = text.chars().count();
            let sized = (min..=max).contains(&length) || (text.is_empty() && default.is_empty());
            sized && (!code || is_code(text))
        }),
        Kind::Time { .. } => value.as_str().is_some_and(is_time),
        Kind::Timezone { .. } => value
            .as_str()
            .is_some_and(|zone| zone.parse::<chrono_tz::Tz>().is_ok()),
        Kind::Schedules { .. } => false,
    }
}

/// How the log shows `value` of `field`: its option's label, a number with its unit, a
/// time as a clock reads it; none for nothing set.
pub(crate) fn shown(
    field: &Field,
    value: &Value,
    options: Option<&[(String, String)]>,
) -> Option<String> {
    let label = |value: &str| {
        options
            .unwrap_or_default()
            .iter()
            .find(|(v, _)| v == value)
            .map_or_else(|| value.to_owned(), |(_, label)| label.clone())
    };
    match field.kind {
        Kind::Toggle { .. } => Some(if value.as_bool()? { "On" } else { "Off" }.to_owned()),
        Kind::Choice { .. } => value.as_str().map(label),
        Kind::Pick { none, .. } => match value.as_str() {
            Some(value) => Some(label(value)),
            None => none.map(str::to_owned),
        },
        Kind::Many { .. } => {
            let labels: Vec<String> = value
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .map(label)
                .collect();
            (!labels.is_empty()).then(|| labels.join(", "))
        }
        Kind::Number {
            unit,
            presets,
            none,
            ..
        } => match value.as_i64() {
            Some(number) => Some(
                presets
                    .iter()
                    .find(|(preset, _)| *preset == number)
                    .map(|(_, label)| (*label).to_owned())
                    .or_else(|| unit.map(|unit| format!("{number} {unit}")))
                    .unwrap_or_else(|| number.to_string()),
            ),
            None => none.map(str::to_owned),
        },
        Kind::Text { .. } | Kind::Timezone { .. } => value
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        Kind::Time { .. } => {
            let (hours, minutes) = value.as_str()?.split_once(':')?;
            let hours: u8 = hours.parse().ok()?;
            let half = if hours < 12 { "AM" } else { "PM" };
            Some(format!("{}:{minutes} {half}", (hours + 11) % 12 + 1))
        }
        Kind::Schedules { .. } => None,
    }
}

/// Whether `text` is a code: uppercase letters and digits.
fn is_code(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
/// Whether `text` is a time of day, `HH:MM`.
pub(crate) fn is_time(text: &str) -> bool {
    text.len() == 5 && chrono::NaiveTime::parse_from_str(text, "%H:%M").is_ok()
}
