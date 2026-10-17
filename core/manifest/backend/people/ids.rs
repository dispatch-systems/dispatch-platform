//! The two things the people slot names by permanent ids: a source of the IDs people are known
//! by, and a kind of collected data people appear in. Each is declared once, as a static
//! `…Info`, and handed around as a small handle compared, hashed, stored, sent and read by its
//! id, in its declared order, and written to TypeScript as the union of the declared ids.
use crate::manifest::registry;
use std::sync::LazyLock;

/// A source of the IDs people are known by, as the owner of those IDs declares it: Paycom's
/// employee codes, Amazon's transporter IDs. Each kind of data naming people by its IDs names
/// it as its source.
pub struct SourceInfo {
    /// Permanent: people's IDs are kept under it.
    pub id: &'static str,
    /// How it is named where people's IDs are listed: "Paycom".
    pub label: &'static str,
    /// Its place in the order sources are listed in everywhere, which is also whose name a
    /// person is shown by: the first source's that knows them. Two in one place go by id.
    pub order: u16,
}
/// A kind of collected data a person can appear in, as the feature whose data it is declares
/// it in its `people`.
pub struct KindInfo {
    /// Permanent: answers about people name it.
    pub id: &'static str,
    /// Its place in the order the kinds are listed in everywhere, which is also the order an
    /// ID's spellings are read in: the first is the one it is shown by. Two in one place go by
    /// id.
    pub order: u16,
    /// How it is named where people are listed: "DVIC".
    pub label: &'static str,
    /// Its icon where people are listed, by the name lucide-react exports it as:
    /// "ClipboardCheck".
    pub icon: &'static str,
}

/// A handle to a declaration: `$handle(&'static $info)`, every one of them read from the
/// registry by `$every`, in their order, and written to TypeScript as `$ts`.
macro_rules! declared {
    ($(#[$doc:meta])* $handle:ident($info:ident), $every:expr, $ts:literal, $what:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy)]
        pub struct $handle(&'static $info);
        impl $handle {
            pub const fn new(info: &'static $info) -> Self {
                Self(info)
            }
            /// Every one declared, in their order.
            pub fn all() -> impl Iterator<Item = Self> {
                static ALL: LazyLock<Vec<$handle>> = LazyLock::new(|| {
                    let mut all: Vec<$handle> = $every;
                    all.sort();
                    all.dedup();
                    all
                });
                ALL.iter().copied()
            }
            /// The declared one `id` names.
            pub fn parse(id: &str) -> Option<Self> {
                Self::all().find(|it| it.0.id == id)
            }
            pub const fn as_str(self) -> &'static str {
                self.0.id
            }
            pub const fn label(self) -> &'static str {
                self.0.label
            }
            pub const fn order(self) -> u16 {
                self.0.order
            }
        }
        impl PartialEq for $handle {
            fn eq(&self, other: &Self) -> bool {
                self.0.id == other.0.id
            }
        }
        impl Eq for $handle {}
        impl std::hash::Hash for $handle {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.0.id.hash(state);
            }
        }
        impl PartialOrd for $handle {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for $handle {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                (self.0.order, self.0.id).cmp(&(other.0.order, other.0.id))
            }
        }
        impl std::fmt::Debug for $handle {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0.id)
            }
        }
        impl serde::Serialize for $handle {
            fn serialize<S: serde::Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(self.0.id)
            }
        }
        impl<'de> serde::Deserialize<'de> for $handle {
            fn deserialize<D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> std::result::Result<Self, D::Error> {
                let id = String::deserialize(deserializer)?;
                Self::parse(&id).ok_or_else(|| serde::de::Error::custom(concat!("unknown ", $what)))
            }
        }
        impl rusqlite::types::ToSql for $handle {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(self.0.id.into())
            }
        }
        /// One no feature in this build declares any more reads as an error: read it as text
        /// where one may be stored.
        impl rusqlite::types::FromSql for $handle {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                value.as_str().and_then(|id| {
                    Self::parse(id).ok_or(rusqlite::types::FromSqlError::InvalidType)
                })
            }
        }
        #[cfg(feature = "ts")]
        impl ts_rs::TS for $handle {
            type WithoutGenerics = Self;
            type OptionInnerType = Self;
            const IS_ENUM: bool = true;
            fn docs() -> Option<String> {
                Some(concat!("/**\n * A ", $what, ", by its id.\n */\n").to_owned())
            }
            fn name(_: &ts_rs::Config) -> String {
                $ts.to_owned()
            }
            // With nothing declared, as in a build without features, nothing is one.
            fn inline(_: &ts_rs::Config) -> String {
                let ids: Vec<_> = Self::all().map(|it| format!("{:?}", it.0.id)).collect();
                if ids.is_empty() {
                    "never".to_owned()
                } else {
                    ids.join(" | ")
                }
            }
            fn decl(cfg: &ts_rs::Config) -> String {
                format!("type {} = {};", $ts, Self::inline(cfg))
            }
            fn decl_concrete(cfg: &ts_rs::Config) -> String {
                Self::decl(cfg)
            }
            fn output_path() -> Option<std::path::PathBuf> {
                Some(concat!("core/manifest/api/generated/", $ts, ".ts").into())
            }
        }
    };
}

declared!(
    /// Where an ID comes from.
    Source(SourceInfo),
    registry().people().iter().map(|kind| kind.source).collect(),
    "PeopleSource",
    "source of IDs"
);
declared!(
    /// A kind of collected data a person can appear in.
    Kind(KindInfo),
    registry().people().iter().map(|kind| kind.kind()).collect(),
    "PeopleKind",
    "kind of collected data"
);
impl Kind {
    pub const fn icon(self) -> &'static str {
        self.0.icon
    }
}
