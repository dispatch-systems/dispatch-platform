use crate::{
    Result,
    db::{FromRow, Row},
    ensure,
    foundation::{config::Environment, validate as v, wire::request},
    text_enum,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
    /// What an agent key may do: look things up, or also run collections and test
    /// connections.
    pub enum AgentAccess {
        Read => "read",
        Operator => "operator",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
    /// A key made on the Agents page, or an app the owner connected with Sign in with Dispatch.
    pub enum AgentKeyKind {
        Key => "key",
        App => "app",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentClient {
    pub name: String,
    pub known: bool,
    pub status: AgentAppStatus,
}

/// A key as the Agents page lists it. The key itself is shown once, when it is made. A
/// connected app is listed the same way; it has no key, so its hint is empty.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentDsp {
    pub id: String,
    pub name: String,
}
/// The Agents page: every key, newest first, and the DSPs a key can be given.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentKeys {
    pub keys: Vec<AgentKey>,
    pub dsps: Vec<AgentDsp>,
}
/// A new key: the key itself, shown this once, and the key as the page lists it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentKeyCreated {
    pub key: AgentKey,
    pub token: String,
}
/// What a new or changed key may do. `expiresAt` is an RFC 3339 time, or null for never.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentKeyRequest {
    pub name: String,
    pub all_dsps: bool,
    pub dsps: Vec<String>,
    pub access: AgentAccess,
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
        Ok(input)
    }
}
/// What the platform owner grants an app they approve: the choices a key is made with,
/// always read-only and never expiring.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OAuthApproval {
    pub name: String,
    pub all_dsps: bool,
    pub dsps: Vec<String>,
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
        Ok(input)
    }
    /// The same choices as a key's, checked by the same rules.
    pub fn key(&self) -> AgentKeyRequest {
        AgentKeyRequest {
            name: self.name.clone(),
            all_dsps: self.all_dsps,
            dsps: self.dsps.clone(),
            access: AgentAccess::Read,
            expires_at: None,
        }
    }
}
/// An app asking the platform owner to connect, as the approval page shows it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct OAuthReplaced {
    pub name: String,
    pub connected_at: String,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct OAuthRedirect {
    pub redirect: String,
}

/// The window in which apps may ask to connect: open until this time, or closed (null).
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct OAuthPairing {
    pub open_until: Option<String>,
}
/// The window just opened or extended, and when it now closes.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct OAuthPairingOpened {
    pub open_until: String,
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct OAuthAllowedApp {
    pub id: OAuthAppId,
    pub name: String,
    pub allowed: bool,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct OAuthAllowedApps {
    pub apps: Vec<OAuthAllowedApp>,
}
/// The platform owner lets a kind of app connect, or stops it.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OAuthAppChoice {
    pub id: OAuthAppId,
    pub allowed: bool,
}

/// How many keys were revoked.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentKeysRevoked {
    pub revoked: usize,
}

/// What `GET /api/v1/whoami` tells an agent: its key, the time, and the DSPs it reaches.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentWhoami {
    pub key: AgentWhoamiKey,
    pub environment: Environment,
    pub now: String,
    pub dsps: Vec<AgentWhoamiDsp>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentWhoamiKey {
    pub name: String,
    pub access: AgentAccess,
    pub expires_at: Option<String>,
}
/// A DSP as an agent sees it: its local date, so "today" and "yesterday" mean the DSP's.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentWhoamiDsp {
    pub id: String,
    pub name: String,
    pub timezone: String,
    pub today: String,
}

/// A key or connected app as the Activity log names it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
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
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    pub at: String,
    pub key: AgentActivityKey,
    pub surface: String,
    pub dsp: Option<AgentDsp>,
    pub outcome: String,
    pub ms: u32,
    pub bytes: u32,
}
/// A page of the Activity log, newest first. `next` is the `before` that reads the page
/// after it; null on the last.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/mcp/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AgentActivityPage {
    pub rows: Vec<AgentActivity>,
    pub next: Option<String>,
}
