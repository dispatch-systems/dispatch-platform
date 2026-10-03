use crate::{
    Result,
    config::Environment,
    db::{FromRow, Row},
    ensure, text_enum, validate as v,
    wire::request,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// What an agent key may do: look things up, or also run collections and test
    /// connections.
    pub enum AgentAccess {
        Read => "read",
        Operator => "operator",
    }
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// A kind of data a key or app may read. `locations` is the delivery addresses and GPS
    /// that route answers carry, and only matters with `routes`.
    pub enum AgentArea {
        Routes => "routes",
        Locations => "locations",
        Timecards => "timecards",
        MealBreaks => "meal_breaks",
        Dvic => "dvic",
        Feedback => "feedback",
        Safety => "safety",
        Returns => "returns",
        Scorecard => "scorecard",
    }
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// A feature switched per DSP that agents read data from: Routes, Timecard, its Meal
    /// Breaks tab, DVIC and Scorecard.
    pub enum AgentSource {
        Routes => "routes",
        Timecards => "timecards",
        MealBreaks => "meal_breaks",
        Dvic => "dvic",
        Scorecard => "scorecard",
    }
}
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

impl AgentArea {
    /// Every kind, in the order they are listed everywhere.
    pub const ALL: [AgentArea; 9] = [
        Self::Routes,
        Self::Locations,
        Self::Timecards,
        Self::MealBreaks,
        Self::Dvic,
        Self::Feedback,
        Self::Safety,
        Self::Returns,
        Self::Scorecard,
    ];
    /// The kind as the Agents page names it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Routes => "Routes & packages",
            Self::Locations => "Delivery addresses & GPS",
            Self::Timecards => "Timecards",
            Self::MealBreaks => "Meal breaks",
            Self::Dvic => "DVIC inspections",
            Self::Feedback => "Customer feedback",
            Self::Safety => "Safety events",
            Self::Returns => "Returns & contact compliance",
            Self::Scorecard => "Weekly scorecard",
        }
    }
    /// The feature it is read from.
    pub const fn source(self) -> AgentSource {
        match self {
            Self::Routes | Self::Locations => AgentSource::Routes,
            Self::Timecards => AgentSource::Timecards,
            Self::MealBreaks => AgentSource::MealBreaks,
            Self::Dvic => AgentSource::Dvic,
            Self::Feedback | Self::Safety | Self::Returns | Self::Scorecard => {
                AgentSource::Scorecard
            }
        }
    }
}
impl AgentSource {
    /// Every feature agents read from, in the order they are listed everywhere.
    pub const ALL: [AgentSource; 5] = [
        Self::Routes,
        Self::Timecards,
        Self::MealBreaks,
        Self::Dvic,
        Self::Scorecard,
    ];
    /// The switch's name on the platform's DSPs page.
    pub const fn switch(self) -> &'static str {
        match self {
            Self::Routes => "Routes",
            Self::Timecards => "Timecard",
            Self::MealBreaks => "Timecard · Meal Breaks",
            Self::Dvic => "DVIC",
            Self::Scorecard => "Scorecard",
        }
    }
}
/// Kinds of data once each, in their order. Delivery addresses come only with the routes, so
/// nothing says it reads them where it can't.
fn canonical(areas: &[AgentArea]) -> Vec<AgentArea> {
    let routes = areas.contains(&AgentArea::Routes);
    AgentArea::ALL
        .into_iter()
        .filter(|area| areas.contains(area) && (routes || *area != AgentArea::Locations))
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
            reads.areas.retain(|area| *area != AgentArea::Locations);
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
