use crate::{
    Result,
    config::Environment,
    db::{FromRow, Row},
    ensure,
    manifest::registry,
    text_enum, validate as v,
    wire::request,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::LazyLock;

text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// What an agent key may do: look things up, or also run collections and test
    /// connections.
    pub enum AgentAccess {
        Read => "read",
        Operator => "operator",
    }
}
/// A kind of data a key or app may read, as the feature that holds it declares it in its
/// manifest's `mcp`.
pub struct ReadToggle {
    /// Permanent: keys, apps and the audit log store it.
    pub id: &'static str,
    /// The kind as the Agents page names it.
    pub label: &'static str,
    /// Its place in the one order the kinds are listed in everywhere.
    pub order: u16,
    /// The feature it is read from.
    pub source: AgentSource,
    /// The kind it comes with and only matters beside, as delivery addresses come with the
    /// routes: allowed only with it, and refused as it is.
    pub with: Option<AgentArea>,
    /// The sources whose IDs it names drivers by.
    pub names: &'static [DriverSource],
}
/// A kind of data a key or app may read. `locations` is the delivery addresses and GPS
/// that route answers carry, and only matters with `routes`.
#[derive(Clone, Copy)]
pub struct AgentArea(&'static ReadToggle);
/// A feature switched per DSP that agents read data from, as it declares itself in its
/// manifest's `mcp`.
pub struct ReadSource {
    /// Permanent: answers name it.
    pub id: &'static str,
    /// The switch's name on the platform's DSPs page.
    pub switch: &'static str,
    /// Its place in the one order the sources are listed in everywhere.
    pub order: u16,
    /// The switches of the catalog that turn it on, any one of them.
    pub features: &'static [&'static str],
}
/// A feature switched per DSP that agents read data from: Routes, Timecard, its Meal
/// Breaks tab, DVIC and Scorecard.
#[derive(Clone, Copy)]
pub struct AgentSource(&'static ReadSource);
/// What a key or app may read: the kinds of data, and whether it bypasses features, reading
/// them even where a DSP has the feature switched off. Bypassing only ever reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentReads {
    pub areas: Vec<AgentArea>,
    pub bypass: bool,
}
/// A DSP's own settings, read in place of the key's or app's own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDspReads {
    pub dsp: String,
    pub areas: Vec<AgentArea>,
    pub bypass: bool,
}

/// Every kind the features declare, in their order.
static AREAS: LazyLock<Vec<AgentArea>> = LazyLock::new(|| {
    let mut all: Vec<AgentArea> = registry()
        .features
        .iter()
        .flat_map(|feature| feature.mcp.reads)
        .copied()
        .collect();
    all.sort_by_key(|area| area.0.order);
    all
});
/// Every source the features declare, in their order.
static SOURCES: LazyLock<Vec<AgentSource>> = LazyLock::new(|| {
    let mut all: Vec<AgentSource> = registry()
        .features
        .iter()
        .flat_map(|feature| feature.mcp.sources)
        .copied()
        .collect();
    all.sort_by_key(|source| source.0.order);
    all
});
impl AgentArea {
    pub const fn new(toggle: &'static ReadToggle) -> Self {
        Self(toggle)
    }
    /// Every kind, in the order they are listed everywhere.
    pub fn all() -> impl Iterator<Item = Self> {
        AREAS.iter().copied()
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::all().find(|area| area.0.id == text)
    }
    pub const fn as_str(self) -> &'static str {
        self.0.id
    }
    pub const fn order(self) -> u16 {
        self.0.order
    }
    /// The kind as the Agents page names it.
    pub const fn label(self) -> &'static str {
        self.0.label
    }
    /// The feature it is read from.
    pub const fn source(self) -> AgentSource {
        self.0.source
    }
    /// The kind it comes with, if it only matters beside one.
    pub const fn with(self) -> Option<AgentArea> {
        self.0.with
    }
    /// Whether it names drivers by `source`'s IDs.
    pub fn names(self, source: DriverSource) -> bool {
        self.0.names.contains(&source)
    }
}
impl AgentSource {
    pub const fn new(source: &'static ReadSource) -> Self {
        Self(source)
    }
    /// Every feature agents read from, in the order they are listed everywhere.
    pub fn all() -> impl Iterator<Item = Self> {
        SOURCES.iter().copied()
    }
    pub const fn as_str(self) -> &'static str {
        self.0.id
    }
    pub const fn order(self) -> u16 {
        self.0.order
    }
    /// The switch's name on the platform's DSPs page.
    pub const fn switch(self) -> &'static str {
        self.0.switch
    }
    /// Whether a DSP whose switches `on` are on has it on.
    pub fn on(self, on: &[String]) -> bool {
        self.0.features.iter().any(|id| on.iter().any(|f| f == id))
    }
}
/// A declared kind or source as everything outside the registry sees it: its id, compared,
/// hashed, sent and read as such, and in TypeScript the union of the declared ids, with the
/// docs the closed enum it replaced had.
macro_rules! declared {
    ($name:ident, $what:literal, $all:ident, $docs:literal) => {
        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.0.id == other.0.id
            }
        }
        impl Eq for $name {}
        impl std::hash::Hash for $name {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.0.id.hash(state);
            }
        }
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0.id)
            }
        }
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(self.0.id)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> std::result::Result<Self, D::Error> {
                let id = String::deserialize(deserializer)?;
                $all.iter()
                    .copied()
                    .find(|named| named.0.id == id)
                    .ok_or_else(|| serde::de::Error::custom(concat!("unknown ", $what)))
            }
        }
        #[cfg(test)]
        impl ts_rs::TS for $name {
            type WithoutGenerics = Self;
            type OptionInnerType = Self;
            const IS_ENUM: bool = true;
            fn docs() -> Option<String> {
                Some($docs.to_owned())
            }
            fn name(_: &ts_rs::Config) -> String {
                stringify!($name).to_owned()
            }
            fn inline(_: &ts_rs::Config) -> String {
                let ids: Vec<_> = $all
                    .iter()
                    .map(|named| format!("{:?}", named.0.id))
                    .collect();
                ids.join(" | ")
            }
            fn decl(cfg: &ts_rs::Config) -> String {
                format!("type {} = {};", stringify!($name), Self::inline(cfg))
            }
            fn decl_concrete(cfg: &ts_rs::Config) -> String {
                Self::decl(cfg)
            }
            fn output_path() -> Option<std::path::PathBuf> {
                Some(concat!(stringify!($name), ".ts").into())
            }
        }
    };
}
declared!(
    AgentArea,
    "kind of data",
    AREAS,
    "/**\n * A kind of data a key or app may read. `locations` is the delivery addresses and \
     GPS\n * that route answers carry, and only matters with `routes`.\n */\n"
);
declared!(
    AgentSource,
    "source",
    SOURCES,
    "/**\n * A feature switched per DSP that agents read data from: Routes, Timecard, its \
     Meal\n * Breaks tab, DVIC and Scorecard.\n */\n"
);
/// The kind of data the `locations` field of a key's row and of an approval stands for,
/// which an older release reads and writes beside `areas`.
pub const OLDER_LOCATIONS: &str = "locations";
/// Kinds of data once each, in their order. One that comes with another, as delivery
/// addresses come with the routes, is kept only beside it, so nothing says it reads them
/// where it can't.
fn canonical(areas: &[AgentArea]) -> Vec<AgentArea> {
    AgentArea::all()
        .filter(|area| areas.contains(area) && area.with().is_none_or(|with| areas.contains(&with)))
        .collect()
}
impl AgentReads {
    /// Reads as the database keeps them: the kinds comma-separated, and bypass as 0 or 1.
    /// A kind this release doesn't know, as a newer one may write, is left out.
    pub fn stored(areas: &str, bypass: i64) -> Self {
        let areas: Vec<AgentArea> = areas.split(',').filter_map(AgentArea::parse).collect();
        Self {
            areas: canonical(&areas),
            bypass: bypass == 1,
        }
    }
    /// A key's or app's own reads as its row keeps them, with the old `locations` column this
    /// release writes to match `areas`. An older release, run again in a rollback, changes only
    /// that column: delivery addresses it stopped are read as stopped.
    pub fn stored_key(areas: &str, bypass: i64, locations: i64) -> Self {
        let mut reads = Self::stored(areas, bypass);
        if locations != 1 {
            reads.areas.retain(|area| area.as_str() != OLDER_LOCATIONS);
        }
        reads
    }
    /// The kinds as the database keeps them.
    pub fn areas_text(&self) -> String {
        canonical(&self.areas)
            .iter()
            .map(|area| area.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
    pub fn has(&self, area: AgentArea) -> bool {
        self.areas.contains(&area)
    }
    /// What the `locations` column an older release reads is written as.
    pub fn locations(&self) -> bool {
        self.areas
            .iter()
            .any(|area| area.as_str() == OLDER_LOCATIONS)
    }
}
impl AgentDspReads {
    /// The settings alone, without their DSP.
    pub fn reads(&self) -> AgentReads {
        AgentReads {
            areas: self.areas.clone(),
            bypass: self.bypass,
        }
    }
}

text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// A key made on the Agents page, or an app the owner connected with Sign in with Dispatch.
    pub enum AgentKeyKind {
        Key => "key",
        App => "app",
    }
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// Whether a connected app can still renew its access. Signed out, it holds no refresh
    /// token left to use, as after 30 days unused, and connects again from the app.
    pub enum AgentAppStatus {
        Connected => "connected",
        SignedOut => "signed_out",
    }
}
/// The app behind a connected app: the name it goes by, whether Dispatch recognizes its
/// published metadata or only has its word (an app that registered itself), and whether
/// it is still signed in.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentClient {
    pub name: String,
    pub known: bool,
    pub status: AgentAppStatus,
}

/// A key as the Agents page lists it. The key itself is shown once, when it is made. A
/// connected app is listed the same way; it has no key, so its hint is empty.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentKey {
    pub id: String,
    pub kind: AgentKeyKind,
    /// The app, for a connected app; null for a key.
    pub client: Option<AgentClient>,
    pub name: String,
    /// The key's last four characters.
    pub hint: String,
    pub access: AgentAccess,
    /// What it reads at every DSP without settings of its own.
    pub reads: AgentReads,
    /// The DSPs with settings of their own.
    pub dsp_reads: Vec<AgentDspReads>,
    /// Every DSP, those added later included; otherwise only `dsps`.
    pub all_dsps: bool,
    pub dsps: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub last_client: Option<String>,
}
impl FromRow for AgentKey {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let kind: AgentKeyKind = row.get("kind")?;
        let client = match kind {
            AgentKeyKind::Key => None,
            AgentKeyKind::App => Some(AgentClient {
                name: row
                    .get::<Option<String>>("client_name")?
                    .unwrap_or_default(),
                known: row.get::<i64>("client_verified")? == 1,
                status: if row.get::<i64>("signed_in")? == 1 {
                    AgentAppStatus::Connected
                } else {
                    AgentAppStatus::SignedOut
                },
            }),
        };
        Ok(Self {
            id: row.get("id")?,
            kind,
            client,
            name: row.get("name")?,
            hint: row.get("hint")?,
            access: row.get("access")?,
            reads: AgentReads::stored_key(
                &row.get::<String>("areas")?,
                row.get("bypass")?,
                row.get("locations")?,
            ),
            dsp_reads: vec![],
            all_dsps: row.get::<i64>("all_dsps")? == 1,
            dsps: vec![],
            created_at: row.get("created_at")?,
            expires_at: row.get("expires_at")?,
            revoked_at: row.get("revoked_at")?,
            last_used_at: row.get("last_used_at")?,
            last_client: row.get("last_client")?,
        })
    }
}
/// A DSP as the Activity log names it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentDsp {
    pub id: String,
    pub name: String,
}
/// A DSP a key can be given, with the features it has switched off that agents read from.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentKeyDsp {
    pub id: String,
    pub name: String,
    pub switched_off: Vec<AgentSource>,
}
/// The Agents page: every key, newest first, and the DSPs a key can be given.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentKeys {
    pub keys: Vec<AgentKey>,
    pub dsps: Vec<AgentKeyDsp>,
}
/// A new key: the key itself, shown this once, and the key as the page lists it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentKeyCreated {
    pub key: AgentKey,
    pub token: String,
}
/// What a new or changed key may do. `expiresAt` is an RFC 3339 time, or null for never.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentKeyRequest {
    pub name: String,
    pub all_dsps: bool,
    pub dsps: Vec<String>,
    pub access: AgentAccess,
    pub reads: AgentReads,
    pub dsp_reads: Vec<AgentDspReads>,
    pub expires_at: Option<String>,
}
impl AgentKeyRequest {
    pub fn parse(value: &Value) -> Result<Self> {
        let mut input: Self = request(value)?;
        input.name = v::name(value, "name", 80)?;
        ensure(input.dsps.len() <= 500, "invalid_input", 400)?;
        input.dsps.sort();
        input.dsps.dedup();
        ensure(
            input.all_dsps == input.dsps.is_empty(),
            "invalid_input",
            400,
        )?;
        input.reads.areas = canonical(&input.reads.areas);
        // A DSP has settings of its own once, kept in the order they are listed back.
        ensure(input.dsp_reads.len() <= 500, "invalid_input", 400)?;
        input.dsp_reads.sort_by(|a, b| a.dsp.cmp(&b.dsp));
        ensure(
            input
                .dsp_reads
                .windows(2)
                .all(|pair| pair[0].dsp != pair[1].dsp),
            "invalid_input",
            400,
        )?;
        for own in &mut input.dsp_reads {
            own.areas = canonical(&own.areas);
        }
        Ok(input)
    }
}
/// What the platform owner grants an app they approve: the choices a key is made with,
/// always read-only and never expiring. A DSP is given settings of its own afterwards, by
/// editing the app.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OAuthApproval {
    pub name: String,
    pub all_dsps: bool,
    pub dsps: Vec<String>,
    pub reads: AgentReads,
}
impl OAuthApproval {
    pub fn parse(value: &Value) -> Result<Self> {
        let mut input: Self = request(value)?;
        input.name = v::name(value, "name", 80)?;
        ensure(
            input.all_dsps == input.dsps.is_empty() && input.dsps.len() <= 500,
            "invalid_input",
            400,
        )?;
        input.reads.areas = canonical(&input.reads.areas);
        Ok(input)
    }
    /// The same choices as a key's, checked by the same rules.
    pub fn key(&self) -> AgentKeyRequest {
        AgentKeyRequest {
            name: self.name.clone(),
            all_dsps: self.all_dsps,
            dsps: self.dsps.clone(),
            access: AgentAccess::Read,
            reads: self.reads.clone(),
            dsp_reads: vec![],
            expires_at: None,
        }
    }
}
/// An app asking the platform owner to connect, as the approval page shows it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OAuthRequest {
    pub id: String,
    pub app: OAuthApp,
    pub expires_at: String,
    /// The connected app approving this one under the name asked about would replace: the
    /// same app connected earlier under that name. Null when nothing would be.
    pub replaces: Option<OAuthReplaced>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OAuthReplaced {
    pub name: String,
    pub connected_at: String,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OAuthApp {
    pub name: String,
    pub client_id: String,
    /// Whether Dispatch recognizes the app's reviewed published metadata. Public app
    /// process identity is not authenticated by this value.
    pub known: bool,
    /// Where the approval is sent: "this computer" for an app on the owner's own computer,
    /// otherwise the host, such as chatgpt.com.
    pub redirect_host: String,
    /// The app's own URI scheme, such as cursor, when it opens an app on this computer by one.
    pub redirect_scheme: Option<String>,
}
/// Where the browser goes next: back to the app, with its code or the owner's refusal.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OAuthRedirect {
    pub redirect: String,
}

/// The window in which apps may ask to connect: open until this time, or closed (null).
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OAuthPairing {
    pub open_until: Option<String>,
}
/// The window just opened or extended, and when it now closes.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OAuthPairingOpened {
    pub open_until: String,
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// A kind of app the platform owner lets connect: one of the four known apps, apps on the
    /// owner's own computer that register themselves, or websites and other apps.
    pub enum OAuthAppId {
        Chatgpt => "chatgpt",
        Codex => "codex",
        ClaudeCode => "claude-code",
        Hermes => "hermes",
        Local => "local",
        Web => "web",
    }
}
/// A kind of app as the Connect tab lists it, and whether it may connect.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OAuthAllowedApp {
    pub id: OAuthAppId,
    pub name: String,
    pub allowed: bool,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OAuthAllowedApps {
    pub apps: Vec<OAuthAllowedApp>,
}
/// The platform owner lets a kind of app connect, or stops it.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OAuthAppChoice {
    pub id: OAuthAppId,
    pub allowed: bool,
}

/// How many keys were revoked.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentKeysRevoked {
    pub revoked: usize,
}

/// What `GET /api/v1/whoami` tells an agent: its key, the time, and the DSPs it reaches.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentWhoami {
    pub key: AgentWhoamiKey,
    pub environment: Environment,
    pub now: String,
    pub dsps: Vec<AgentWhoamiDsp>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentWhoamiKey {
    pub name: String,
    pub access: AgentAccess,
    pub expires_at: Option<String>,
}
/// A DSP as an agent sees it: its local date, so "today" and "yesterday" mean the DSP's,
/// the features it has switched on, and what this key or app reads there.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentWhoamiDsp {
    pub id: String,
    pub name: String,
    pub timezone: String,
    pub today: String,
    pub features: Vec<String>,
    pub reads: AgentReads,
}

/// A key or connected app as the Activity log names it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentActivityKey {
    pub id: String,
    pub name: String,
    pub kind: AgentKeyKind,
}
/// One call an agent made: when it started, with which key or app, to which endpoint
/// (`rest:<endpoint>`) or tool (`mcp:<tool>`), about which DSP, and how it ended: `ok`, or
/// the code it was refused or failed with. `ms` is how long it took, `bytes` how much it
/// answered. A key's calls past 10,000 in a UTC day are not kept: one row with surface
/// `activity:capped` and outcome `capped`, at the first of them, marks the day capped.
/// `bypassed` is whether it read a feature the DSP has switched off, by bypassing features.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    pub at: String,
    pub key: AgentActivityKey,
    pub surface: String,
    pub dsp: Option<AgentDsp>,
    pub outcome: String,
    pub ms: u32,
    pub bytes: u32,
    pub bypassed: bool,
}
/// A page of the Activity log, newest first. `next` is the `before` that reads the page
/// after it; null on the last.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentActivityPage {
    pub rows: Vec<AgentActivity>,
    pub next: Option<String>,
}
// A4: the identity sources, until the features that hold them declare them.
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    #[derive(PartialOrd, Ord)]
    /// Where an ID comes from. Every Amazon source (routes, meal breaks, DVIC, the
    /// scorecard) knows a driver by the same transporter ID.
    pub enum DriverSource {
        Paycom => "paycom",
        Amazon => "amazon",
    }
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// Where a person stands. `review` is a person who might be someone else listed twice;
    /// `former` someone only one source knows who has left.
    pub enum DriverStatus {
        Matched => "matched",
        Variant => "variant",
        Confirmed => "confirmed",
        Review => "review",
        PaycomOnly => "paycom_only",
        AmazonOnly => "amazon_only",
        Office => "office",
        Former => "former",
    }
}
