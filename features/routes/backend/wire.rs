//! An itinerary's response read straight into the fields shaping reads of its stops and
//! tasks, which are most of a response, without a JSON tree of them; the rest of its detail
//! is small and read whole into a tree. Each field reads as a tree would answer it, so the
//! rows are the same: a field missing or of a kind shaping doesn't read is null, one repeated
//! is the last, and anything but an object where one is expected reads as an empty object.
use super::{StopFields, TaskFields};
use serde::de::{Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value, json};
use std::{borrow::Cow, collections::BTreeSet, fmt};

/// An itinerary's response, as shaping reads it.
pub(super) fn read(text: &str) -> serde_json::Result<Itinerary> {
    serde_json::from_str(text)
}

/// The visitor methods of a reading that takes anything but what it expects as nothing.
macro_rules! otherwise_default {
    ($value:ty) => {
        fn visit_bool<E>(self, _: bool) -> Result<$value, E> {
            Ok(Default::default())
        }
        fn visit_i64<E>(self, _: i64) -> Result<$value, E> {
            Ok(Default::default())
        }
        fn visit_u64<E>(self, _: u64) -> Result<$value, E> {
            Ok(Default::default())
        }
        fn visit_f64<E>(self, _: f64) -> Result<$value, E> {
            Ok(Default::default())
        }
        fn visit_str<E>(self, _: &str) -> Result<$value, E> {
            Ok(Default::default())
        }
        fn visit_unit<E>(self) -> Result<$value, E> {
            Ok(Default::default())
        }
    };
}
/// An object read field by field into a struct: each key named into its field, the last when
/// repeated; any other key handed to `$rest`, which keeps what it wants of it in fields of
/// its own, or skipped.
macro_rules! wire {
    (
        $(#[$doc:meta])* $name:ident { $($key:literal => $field:ident: $kind:ty,)* }
        $(else $rest:path => { $($own:ident: $own_kind:ty,)* })?
    ) => {
        $(#[$doc])*
        #[derive(Default)]
        pub(super) struct $name {
            $(pub(super) $field: $kind,)*
            $($(pub(super) $own: $own_kind,)*)?
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Fields;
                impl<'de> Visitor<'de> for Fields {
                    type Value = $name;
                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("any JSON value")
                    }
                    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<$name, A::Error> {
                        let mut read = $name::default();
                        while let Some(Key(key)) = map.next_key()? {
                            match key.as_ref() {
                                $($key => read.$field = map.next_value()?,)*
                                _ => wire!(@rest read, key, map $(, $rest)?),
                            }
                        }
                        Ok(read)
                    }
                    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<$name, A::Error> {
                        while seq.next_element::<IgnoredAny>()?.is_some() {}
                        Ok(Default::default())
                    }
                    otherwise_default!($name);
                }
                deserializer.deserialize_any(Fields)
            }
        }
    };
    (@rest $read:ident, $key:ident, $map:ident) => {{
        $map.next_value::<IgnoredAny>()?;
    }};
    (@rest $read:ident, $key:ident, $map:ident, $rest:path) => {
        $rest(&mut $read, $key, &mut $map)?
    };
}

/// An object key, borrowed from the text where it has no escapes.
struct Key<'de>(Cow<'de, str>);
impl<'de> Deserialize<'de> for Key<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Name;
        impl<'de> Visitor<'de> for Name {
            type Value = Key<'de>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object key")
            }
            fn visit_borrowed_str<E>(self, key: &'de str) -> Result<Key<'de>, E> {
                Ok(Key(Cow::Borrowed(key)))
            }
            fn visit_str<E>(self, key: &str) -> Result<Key<'de>, E> {
                Ok(Key(Cow::Owned(key.to_owned())))
            }
        }
        deserializer.deserialize_str(Name)
    }
}
/// A field shaping reads as text, a number, a time or a flag: as the tree holds it, but an
/// array or object, which those read as nothing, as null.
#[derive(Default)]
pub(super) struct Scalar(Value);
impl<'de> Deserialize<'de> for Scalar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Read;
        impl<'de> Visitor<'de> for Read {
            type Value = Scalar;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_bool<E>(self, value: bool) -> Result<Scalar, E> {
                Ok(Scalar(Value::Bool(value)))
            }
            fn visit_i64<E>(self, value: i64) -> Result<Scalar, E> {
                Ok(Scalar(Value::Number(value.into())))
            }
            fn visit_u64<E>(self, value: u64) -> Result<Scalar, E> {
                Ok(Scalar(Value::Number(value.into())))
            }
            fn visit_f64<E>(self, value: f64) -> Result<Scalar, E> {
                Ok(Scalar(
                    Number::from_f64(value).map_or(Value::Null, Value::Number),
                ))
            }
            fn visit_str<E>(self, value: &str) -> Result<Scalar, E> {
                Ok(Scalar(Value::String(value.to_owned())))
            }
            fn visit_unit<E>(self) -> Result<Scalar, E> {
                Ok(Scalar(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Scalar, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Scalar(Value::Null))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Scalar, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Scalar(Value::Null))
            }
        }
        deserializer.deserialize_any(Read)
    }
}
/// An object as `T` reads it, told apart from anything else, which is none.
pub(super) struct Object<T>(pub(super) Option<T>);
impl<T> Default for Object<T> {
    fn default() -> Self {
        Self(None)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Read<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for Read<T> {
            type Value = Object<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Object<T>, A::Error> {
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(|read| Object(Some(read)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Object<T>, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Object::default())
            }
            otherwise_default!(Object<T>);
        }
        deserializer.deserialize_any(Read(std::marker::PhantomData))
    }
}
/// A list of what shaping reads of each element; anything but an array, as none.
pub(super) struct List<T>(pub(super) Vec<T>);
impl<T> Default for List<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for List<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Elements<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for Elements<T> {
            type Value = List<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<List<T>, A::Error> {
                let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(List(items))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<List<T>, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(List::default())
            }
            otherwise_default!(List<T>);
        }
        deserializer.deserialize_any(Elements(std::marker::PhantomData))
    }
}

wire!(
    /// What shaping reads of an itinerary's response: its detail, the addresses its stops
    /// are at and the drivers it names.
    Itinerary {
        "itineraryDetails" => detail: Object<Detail>,
        "addresses" => addresses: Value,
        "transporters" => transporters: Value,
    }
);
wire!(
    /// The detail: its stops and removed tasks as shaping reads them, and its route-level
    /// fields whole, in `rest`.
    Detail {
        "stops" => stops: List<Stop>,
        "inactiveTasks" => inactive: List<Task>,
    }
    else rest_of_detail => { rest: Map<String, Value>, }
);
/// A detail's route-level field, kept whole.
fn rest_of_detail<'de, A: MapAccess<'de>>(
    detail: &mut Detail,
    key: Cow<'de, str>,
    map: &mut A,
) -> Result<(), A::Error> {
    detail.rest.insert(key.into_owned(), map.next_value()?);
    Ok(())
}
wire!(
    /// What shaping reads of a stop: its fields, its tasks, and the flags set on it.
    Stop {
        "stopId" => stop_id: Scalar,
        "sequenceNumber" => sequence_number: Scalar,
        "addressId" => address_id: Scalar,
        "itineraryStopType" => itinerary_stop_type: Scalar,
        "routeCode" => route_code: Scalar,
        "plannedStartTime" => planned_start_time: Scalar,
        "plannedEndTime" => planned_end_time: Scalar,
        "expectedStartTime" => expected_start_time: Scalar,
        "actualStartTime" => actual_start_time: Scalar,
        "tasks" => tasks: List<Task>,
    }
    else flag_of_stop => { flags: BTreeSet<String>, }
);
/// A stop's flag, named for one or by `is`, kept while it's set: one set again later as
/// unset is unset, as the tree holds it.
fn flag_of_stop<'de, A: MapAccess<'de>>(
    stop: &mut Stop,
    key: Cow<'de, str>,
    map: &mut A,
) -> Result<(), A::Error> {
    if !(key.ends_with("Flag") || key.starts_with("is")) {
        map.next_value::<IgnoredAny>()?;
    } else if map.next_value::<Value>()? == Value::Bool(true) {
        stop.flags.insert(key.into_owned());
    } else {
        stop.flags.remove(key.as_ref());
    }
    Ok(())
}
wire!(
    /// What shaping reads of a task.
    Task {
        "taskId" => task_id: Scalar,
        "transporterId" => transporter_id: Scalar,
        "domainMap" => domain_map: Package,
        "scannableId" => scannable_id: Scalar,
        "taskType" => task_type: Scalar,
        "taskState" => task_state: Scalar,
        "taskStateContext" => task_state_context: Scalar,
        "executionStatus" => execution_status: Scalar,
        "addressId" => address_id: Scalar,
        "windowStartTime" => window_start_time: Scalar,
        "windowEndTime" => window_end_time: Scalar,
        "actualExecutionTime" => actual_execution_time: Scalar,
        "taskExecutionTime" => task_execution_time: Scalar,
        "executionGeocode" => execution_geocode: Geocode,
        "boxType" => box_type: Scalar,
        "weight" => weight: Scalar,
        "weightUnit" => weight_unit: Scalar,
        "length" => length: Scalar,
        "width" => width: Scalar,
        "height" => height: Scalar,
        "volumeUnit" => volume_unit: Scalar,
        "timeWindowed" => time_windowed: Scalar,
        "highValueTaskFlag" => high_value_task_flag: Scalar,
        "customerReturn" => customer_return: Scalar,
        "addressTypeInfo" => address_type_info: Scalar,
        "recentTaskEvents" => recent_task_events: List<Event>,
    }
);
wire!(
    /// The package a task moves.
    Package {
        "scannableId" => scannable_id: Scalar,
        "orderId" => order_id: Scalar,
    }
);
wire!(
    /// Where a task was done.
    Geocode {
        "latitude" => latitude: Scalar,
        "longitude" => longitude: Scalar,
    }
);
wire!(
    /// One of a task's recent events, its three fields whole as they go into its row.
    Event {
        "executionTime" => execution_time: Value,
        "taskState" => task_state: Value,
        "taskStateContext" => task_state_context: Value,
    }
);

impl StopFields for Stop {
    type Task = Task;
    fn field(&self, key: &str) -> &Value {
        let field = match key {
            "stopId" => &self.stop_id,
            "sequenceNumber" => &self.sequence_number,
            "addressId" => &self.address_id,
            "itineraryStopType" => &self.itinerary_stop_type,
            "routeCode" => &self.route_code,
            "plannedStartTime" => &self.planned_start_time,
            "plannedEndTime" => &self.planned_end_time,
            "expectedStartTime" => &self.expected_start_time,
            "actualStartTime" => &self.actual_start_time,
            _ => unread(key),
        };
        &field.0
    }
    fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.0.iter()
    }
    fn flags(&self) -> Value {
        json!(self.flags)
    }
}
impl TaskFields for Task {
    fn field(&self, key: &str) -> &Value {
        let field = match key {
            "taskId" => &self.task_id,
            "transporterId" => &self.transporter_id,
            "scannableId" => &self.scannable_id,
            "taskType" => &self.task_type,
            "taskState" => &self.task_state,
            "taskStateContext" => &self.task_state_context,
            "executionStatus" => &self.execution_status,
            "addressId" => &self.address_id,
            "windowStartTime" => &self.window_start_time,
            "windowEndTime" => &self.window_end_time,
            "actualExecutionTime" => &self.actual_execution_time,
            "taskExecutionTime" => &self.task_execution_time,
            "boxType" => &self.box_type,
            "weight" => &self.weight,
            "weightUnit" => &self.weight_unit,
            "length" => &self.length,
            "width" => &self.width,
            "height" => &self.height,
            "volumeUnit" => &self.volume_unit,
            "timeWindowed" => &self.time_windowed,
            "highValueTaskFlag" => &self.high_value_task_flag,
            "customerReturn" => &self.customer_return,
            "addressTypeInfo" => &self.address_type_info,
            _ => unread(key),
        };
        &field.0
    }
    fn inner(&self, object: &str, key: &str) -> &Value {
        let field = match (object, key) {
            ("domainMap", "scannableId") => &self.domain_map.scannable_id,
            ("domainMap", "orderId") => &self.domain_map.order_id,
            ("executionGeocode", "latitude") => &self.execution_geocode.latitude,
            ("executionGeocode", "longitude") => &self.execution_geocode.longitude,
            _ => unread(key),
        };
        &field.0
    }
    fn events(&self) -> Value {
        Value::Array(
            self.recent_task_events
                .0
                .iter()
                .map(|e| json!([e.execution_time, e.task_state, e.task_state_context]))
                .collect(),
        )
    }
}
/// A field shaping reads that this reading doesn't: a mistake, made plain at once.
fn unread(key: &str) -> &'static Scalar {
    unreachable!("shaping reads {key}, which the itinerary's reading skips")
}
