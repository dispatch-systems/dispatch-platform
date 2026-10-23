//! What Driver Match's endpoints answer with. Each type is written to TypeScript in
//! `generated/`, which `client.ts` imports.
/// What a person does for the DSP, and the title someone gave them, as core's directory says.
pub use dispatch_core::manifest::people::{PersonRole, PersonTitle};
use dispatch_core::{
    manifest::people::{Kind, Source},
    text_enum,
};
use serde::Serialize;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
    /// How an ID came to its person, surest last, as they are ordered: as its first ID, by a
    /// name variant, a unique name, a phone number or an email address only they share, as
    /// another account its source says is theirs, or someone's decision.
    #[derive(PartialOrd, Ord)]
    pub enum IdLink {
        New => "new",
        Variant => "variant",
        Name => "name",
        Phone => "phone",
        Email => "email",
        Account => "account",
        Person => "person",
    }
}
impl IdLink {
    /// Whether a person may confirm the link: one a match made by a name or a contact.
    pub fn confirmable(self) -> bool {
        matches!(self, Self::Variant | Self::Name | Self::Phone | Self::Email)
    }
    /// Whether a person decided it, rather than a match.
    pub fn decided(self) -> bool {
        self == Self::Person
    }
}
/// A role someone gave a person, whatever their sources say: a driver, a helper, offboarded,
/// or management under a title. Stored and sent as the role's id, or the title's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GivenRole {
    Driver,
    Helper,
    Offboarded,
    Title(PersonTitle),
}
impl GivenRole {
    /// Every one, in the order they are offered: the roles, then the titles.
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Driver, Self::Helper, Self::Offboarded]
            .into_iter()
            .chain(PersonTitle::ALL.map(Self::Title))
    }
    /// What they do for the DSP once given it: management, for a title.
    pub const fn role(self) -> PersonRole {
        match self {
            Self::Driver => PersonRole::Driver,
            Self::Helper => PersonRole::Helper,
            Self::Offboarded => PersonRole::Offboarded,
            Self::Title(_) => PersonRole::Office,
        }
    }
    pub const fn title(self) -> Option<PersonTitle> {
        match self {
            Self::Title(title) => Some(title),
            _ => None,
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Title(title) => title.as_str(),
            role => role.role().as_str(),
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::all().find(|given| given.as_str() == text)
    }
    /// The role as people read it: "Driver", "OPs Manager".
    pub const fn label(self) -> &'static str {
        match self {
            Self::Driver => "Driver",
            Self::Helper => "Helper",
            Self::Offboarded => "Offboarded",
            Self::Title(title) => title.label(),
        }
    }
}
impl Serialize for GivenRole {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl rusqlite::types::ToSql for GivenRole {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}
impl rusqlite::types::FromSql for GivenRole {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value
            .as_str()
            .and_then(|text| Self::parse(text).ok_or(rusqlite::types::FromSqlError::InvalidType))
    }
}
// Its TypeScript is the union of every one's id, in their order.
#[cfg(feature = "ts")]
impl ts_rs::TS for GivenRole {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    const IS_ENUM: bool = true;
    fn docs() -> Option<String> {
        Some("/**\n * A role someone gave a person, by its id or its title's.\n */\n".to_owned())
    }
    fn name(_: &ts_rs::Config) -> String {
        "GivenRole".to_owned()
    }
    fn inline(_: &ts_rs::Config) -> String {
        let ids: Vec<_> = Self::all()
            .map(|given| format!("{:?}", given.as_str()))
            .collect();
        ids.join(" | ")
    }
    fn decl(cfg: &ts_rs::Config) -> String {
        format!("type GivenRole = {};", Self::inline(cfg))
    }
    fn decl_concrete(cfg: &ts_rs::Config) -> String {
        Self::decl(cfg)
    }
    fn output_path() -> Option<std::path::PathBuf> {
        Some("features/driver_match/api/generated/GivenRole.ts".into())
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
    /// How likely a pair is one person.
    pub enum Strength {
        Strong => "strong",
        Possible => "possible",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
    /// Why two people might be one.
    pub enum EvidenceKind {
        SameEmail => "same_email",
        SamePhone => "same_phone",
        SameLastName => "same_last_name",
        SameFirstName => "same_first_name",
        ShortForm => "short_form",
        LastInitial => "last_initial",
        WorkedDays => "worked_days",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
    /// What happened in a person's history.
    pub enum PersonEventKind {
        Added => "added",
        Linked => "linked",
        Merged => "merged",
        Split => "split",
        Apart => "apart",
    }
}
/// One ID a person is known by, as its source wrote it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PersonId {
    pub source: Source,
    pub id: String,
    pub name: String,
    pub department: Option<String>,
    /// The email addresses and phone numbers its source has for them.
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    /// What its source says they do: `Driver`, `Helper`.
    pub roles: Vec<String>,
    /// On its source's current roster.
    pub listed: bool,
    /// Its source says the DSP offboarded them.
    pub offboarded: bool,
    /// Whether a photo of them is held under this ID.
    pub photo: bool,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub linked_by: IdLink,
    /// Whether a person may confirm how it came to them.
    pub confirmable: bool,
}
/// A person: their code, what they do and every ID they hold.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub code: String,
    pub name: String,
    pub role: PersonRole,
    /// Their title, while they are still there.
    pub title: Option<PersonTitle>,
    /// The role someone gave them, which `role` follows unless they have left.
    pub given: Option<GivenRole>,
    pub ids: Vec<PersonId>,
    /// Their photo's version, when a feature holds one: its address carries it.
    pub photo: Option<String>,
}
/// How many questions wait, for the sidebar's count.
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct MatchCounts {
    pub questions: usize,
}
/// One reason two people might be one, and whether it argues for them being one. `short` and
/// `long` name a short form and the name it stands for; `same` of `of` days count the days
/// both worked.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub short: Option<String>,
    pub long: Option<String>,
    pub same: Option<usize>,
    pub of: Option<usize>,
    pub supports: bool,
}
/// A question waiting for a person, about people by their codes.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Question {
    /// Two people who may be one: `person` would join `candidate`, keeping `candidate`'s code.
    Pair {
        person: String,
        candidate: String,
        strength: Strength,
        evidence: Vec<Evidence>,
    },
    /// An ID a name written another way joined to `person`, which nobody has confirmed.
    Variant {
        person: String,
        source: Source,
        id: String,
    },
}
/// The Driver Match page: the sources the DSP has, the questions waiting, then everyone.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriverMatch {
    /// When collected data was last checked for new IDs.
    pub checked_at: Option<String>,
    /// The sources of IDs the DSP's features have on, in their order.
    pub sources: Vec<Source>,
    /// The pairs, the strongest first, then the name variants of anyone still there.
    pub questions: Vec<Question>,
    pub people: Vec<Person>,
}
/// How much one kind of data has of a person, in its unit.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub data: Kind,
    pub count: usize,
}
/// One source's last fourteen days of a person's work: for each day, whether they worked,
/// `None` where nothing was collected.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SourceDays {
    pub source: Source,
    pub worked: Vec<Option<bool>>,
}
/// One line of a person's history: an ID added or linked, or a decision with another person.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PersonEvent {
    pub at: String,
    pub kind: PersonEventKind,
    pub source: Option<Source>,
    pub link: Option<IdLink>,
    /// The ID's name for `added` and `linked`; the other person's for `split` and `apart`.
    pub name: Option<String>,
    pub code: Option<String>,
    pub actor: Option<String>,
}
/// One person in full, for their sheet.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PersonDetails {
    pub person: Person,
    /// What they appear in, in the kinds' order.
    pub appears: Vec<Appearance>,
    /// The last fourteen days, oldest first.
    pub dates: Vec<String>,
    /// What each source that saw them work says of those days.
    pub days: Vec<SourceDays>,
    pub history: Vec<PersonEvent>,
}
