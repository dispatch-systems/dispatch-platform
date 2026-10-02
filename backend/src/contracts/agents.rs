use super::*;

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
    /// The tools an agent is offered: all of them, or a few for small models.
    pub enum AgentTools {
        Full => "full",
        Essential => "essential",
    }
}

/// A key as the Agents page lists it. The key itself is shown once, when it is made.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentKey {
    pub id: String,
    pub name: String,
    /// The key's last four characters.
    pub hint: String,
    pub access: AgentAccess,
    pub tools: AgentTools,
    /// Whether answers carry delivery addresses and GPS.
    pub locations: bool,
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
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            hint: row.get("hint")?,
            access: row.get("access")?,
            tools: row.get("tools")?,
            locations: row.get::<i64>("locations")? == 1,
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
/// A DSP a key can be given.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentDsp {
    pub id: String,
    pub name: String,
}
/// The Agents page: every key, newest first, and the DSPs a key can be given.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentKeys {
    pub keys: Vec<AgentKey>,
    pub dsps: Vec<AgentDsp>,
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
    pub tools: AgentTools,
    pub locations: bool,
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
    pub tools: AgentTools,
    pub locations: bool,
    pub expires_at: Option<String>,
}
/// A DSP as an agent sees it: its local date, so "today" and "yesterday" mean the DSP's,
/// and the features it has switched on.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentWhoamiDsp {
    pub id: String,
    pub name: String,
    pub timezone: String,
    pub today: String,
    pub features: Vec<String>,
}
