use crate::{
    Result,
    db::{Store, at, iso, n, now, s},
    manifest::registry,
    tenancy::api::audit::AuditPage,
};
use serde_json::{Value, json};
use std::sync::LazyLock;
const EXPORT_LIMIT: i64 = 50_000;
const VISIT_WINDOW: i64 = 30 * 60 * 1000;
// Activity older than a year is removed by the collector's periodic cleanup.
const AUDIT_RETENTION: i64 = 365 * 24 * 60 * 60 * 1000;
// Managing a DSP from the platform is never part of that DSP's own log.
const PLATFORM_ONLY: [&str; 13] = [
    "dsp.created",
    "dsp.code_changed",
    "dsp.feature_enabled",
    "dsp.feature_disabled",
    "dsp.feature_hidden",
    "dsp.feature_shown",
    "dsp.removed",
    "dsp.restored",
    "dsp.suspended",
    "dsp.resumed",
    "dsp.support_visibility_changed",
    "diagnostics.fixtures_loaded",
    "development.fixtures_loaded",
];
impl Store {
    pub fn audit(
        &self,
        actor: Option<&str>,
        dsp: Option<&str>,
        action: &str,
        detail: &str,
    ) -> Result<()> {
        self.audit_with(actor, dsp, action, detail, None, &[])
    }
    // Names are stored as text so the event still reads after its subject is deleted.
    pub fn audit_with(
        &self,
        actor: Option<&str>,
        dsp: Option<&str>,
        action: &str,
        detail: &str,
        target: Option<&str>,
        changes: &[AuditChange],
    ) -> Result<()> {
        self.audit_ref(actor, dsp, action, detail, target, changes, None)
    }
    // A reference names the record an event is about, so the log can gather
    // everything involving it even after a rename.
    #[allow(clippy::too_many_arguments)]
    pub fn audit_ref(
        &self,
        actor: Option<&str>,
        dsp: Option<&str>,
        action: &str,
        detail: &str,
        target: Option<&str>,
        changes: &[AuditChange],
        reference: Option<(&str, &str)>,
    ) -> Result<()> {
        let data = (target.is_some() || !changes.is_empty() || reference.is_some()).then(|| {
            json!({"target":target,"ref":reference.map(|(kind,id)|json!({"kind":kind,"id":id})),
                "changes":changes.iter().map(|(field,from,to)|json!({"field":field,"from":from,"to":to})).collect::<Vec<_>>()}).to_string()
        });
        let shown = self.shown(actor, dsp, action)?;
        // A platform owner is known by their account; one of a DSP's people by an id only
        // its directory has, with their name kept beside it.
        let member = match (actor, dsp) {
            (Some(actor), _) if self.platform_owner(actor)? => None,
            (Some(actor), Some(dsp)) => Some((actor, self.member_name(dsp, actor)?)),
            (Some(actor), None) => Some((actor, None)),
            (None, _) => None,
        };
        let (actor_id, member_id, name) = match member {
            Some((id, name)) => (None, Some(id), name),
            None => (actor, None, None),
        };
        self.platform.exec(
            "INSERT INTO audit(at,actor_id,member_id,actor_name,dsp_id,action,detail,data,shown) \
             VALUES (?,?,?,?,?,?,?,?,?)",
            rusqlite::params![
                iso(),
                actor_id,
                member_id,
                name,
                dsp,
                action,
                detail,
                data,
                shown.then_some(1)
            ],
        )?;
        Ok(())
    }
    /// Records what someone did to their own account, outside any DSP's log: a platform
    /// owner's under their account, one of a DSP's people's under their id and name.
    pub fn audit_account(
        &self,
        user: &crate::accounts::api::types::PublicUser,
        dsp: Option<&str>,
        action: &str,
        detail: &str,
    ) -> Result<()> {
        let (actor_id, member_id, name) = match dsp {
            None => (Some(user.id.as_str()), None, None),
            Some(_) => (None, Some(user.id.as_str()), Some(user.name())),
        };
        self.platform.exec(
            "INSERT INTO audit(at,actor_id,member_id,actor_name,action,detail) \
             VALUES (?,?,?,?,?,?)",
            rusqlite::params![iso(), actor_id, member_id, name, action, detail],
        )?;
        Ok(())
    }
    /// The name of `dsp`'s person `id`, while their account lasts.
    fn member_name(&self, dsp: &str, id: &str) -> Result<Option<String>> {
        let Ok(people) = self.dsp(dsp) else {
            return Ok(None);
        };
        let name: Option<(String,)> = people.one_as(
            "SELECT first_name||' '||last_name FROM users WHERE id=?",
            [id],
        )?;
        Ok(name.map(|(name,)| name))
    }
    // Decided as the event is written, so a visit made while hidden stays hidden.
    fn shown(&self, actor: Option<&str>, dsp: Option<&str>, action: &str) -> Result<bool> {
        Ok(match (actor, dsp) {
            (Some(actor), Some(dsp)) if !PLATFORM_ONLY.contains(&action) => {
                self.platform_owner(actor)? && self.support_visible(dsp)
            }
            _ => false,
        })
    }
    // Opening a DSP happens on every load; one entry per half hour says as much.
    // A hidden visit does not stand in for one the DSP would now be shown.
    pub fn audit_visit(&self, actor: &str, dsp: &str, action: &str, detail: &str) -> Result<()> {
        let shown = self.shown(Some(actor), Some(dsp), action)?;
        let recent = self.platform.one(
            "SELECT 1 FROM audit WHERE COALESCE(actor_id,member_id)=? AND dsp_id=? AND action=? \
             AND detail=? AND at>=? AND COALESCE(shown,0)=? LIMIT 1",
            rusqlite::params![actor, dsp, action, detail, at(now() - VISIT_WINDOW), shown],
        )?;
        if recent.is_some() {
            return Ok(());
        }
        self.audit(Some(actor), Some(dsp), action, detail)
    }
    // Taking a copy of everyone's activity is itself recorded, after the copy is
    // read so an export never lists itself.
    pub fn audit_export(&self, actor: &str, query: AuditQuery) -> Result<AuditPage> {
        let page = self.audit_page(&AuditQuery {
            before: 0,
            limit: EXPORT_LIMIT,
            ..query
        })?;
        let rows = page.events.len();
        let scope = if query.within.is_empty() {
            query.dsp
        } else {
            Some(query.within)
        };
        self.audit(Some(actor), scope, "audit.exported", &rows.to_string())?;
        Ok(page)
    }
    pub fn prune_audit(&self) -> Result<usize> {
        self.platform.exec(
            "DELETE FROM audit WHERE at<?",
            [at(now() - AUDIT_RETENTION)],
        )
    }
    pub fn platform_owner(&self, user: &str) -> Result<bool> {
        Ok(self
            .platform
            .one(
                "SELECT 1 FROM users WHERE id=? AND platform_owner=1",
                [user],
            )?
            .is_some())
    }
    pub fn support_visible(&self, dsp: &str) -> bool {
        self.profile(dsp)
            .is_ok_and(|profile| profile.support_visible)
    }
    // A DSP's log lists its members' and the system's actions. A platform owner's
    // appear only where the DSP shows Platform support, and never under their name.
    pub fn audit_page(&self, query: &AuditQuery) -> Result<AuditPage> {
        // Inside a DSP a platform owner is only ever "Platform support".
        const SUPPORT: &str = "(?1 IS NOT NULL AND COALESCE(u.platform_owner,0)=1)";
        const FROM: &str = "FROM audit a LEFT JOIN users u ON u.id=a.actor_id LEFT JOIN dsps \
            d ON d.id=a.dsp_id WHERE (?1 IS NULL OR (a.dsp_id=?1 AND (COALESCE(u.platform_owner,0)=0 OR a.shown=1)))";
        let name = format!(
            "CASE WHEN {SUPPORT} THEN 'Platform support' ELSE COALESCE(u.first_name||' '||u.last_name,a.actor_name,'System') END"
        );
        let actor = format!(
            "CASE WHEN {SUPPORT} THEN 'support' ELSE COALESCE(a.actor_id,a.member_id,CASE WHEN \
                a.actor_name IS NULL THEN 'system' ELSE 'name:'||a.actor_name END) END"
        );
        let areas = areas();
        const FAILED: &str = "a.action LIKE '%.failed'";
        let filters = format!(
            "{FROM} AND (?2='' OR a.at>=?2) AND (?3='' OR {actor}=?3) AND (?4='' OR a.action \
                LIKE ?4 ESCAPE '\\' OR a.detail LIKE ?4 ESCAPE '\\' OR COALESCE(a.data,'') \
                LIKE ?4 ESCAPE '\\' OR {name} LIKE ?4 ESCAPE '\\' OR COALESCE(d.name,'') \
                LIKE ?4 ESCAPE '\\') AND (?5='' OR a.dsp_id=?5) AND ((?6='' AND ?7='') OR \
                (?6<>'' AND json_extract(a.data,'$.ref.kind')||':'||json_extract(a.data,'$.ref.id')=?6) OR \
                (?7<>'' AND json_extract(a.data,'$.target')=?7))"
        );
        let area = format!("(?8='' OR (?8='failures' AND {FAILED}) OR {areas}=?8)");
        let search = if query.q.is_empty() {
            String::new()
        } else {
            format!(
                "%{}%",
                query
                    .q
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        };
        let mut events = self.platform.all(
            &format!(
                "SELECT a.id,a.at,CASE WHEN {SUPPORT} \
            THEN NULL ELSE COALESCE(a.actor_id,a.member_id) END actorId,{name} actorName,a.dsp_id dspId,d.name \
            dspName,a.action,a.detail,a.data,{areas} area {filters} AND {area} AND (?9=0 OR \
            a.id<?9) ORDER BY a.id DESC LIMIT \
            ?10"
            ),
            rusqlite::params![
                query.dsp,
                query.from,
                query.actor,
                search,
                query.within,
                query.subject,
                query.named,
                query.area,
                query.before,
                query.limit
            ],
        )?;
        for event in &mut events {
            let data = event["data"]
                .as_str()
                .and_then(|data| serde_json::from_str::<Value>(data).ok())
                .unwrap_or(Value::Null);
            event["target"] = data["target"].clone();
            event["ref"] = data["ref"].clone();
            event["changes"] = if data["changes"].is_array() {
                data["changes"].clone()
            } else {
                json!([])
            };
            event.as_object_mut().unwrap().remove("data");
        }
        for names in registry().features.iter().filter_map(|f| f.audit.names) {
            names(self, &mut events);
        }
        let total = self.platform.count(
            &format!("SELECT count(*) {filters} AND {area}"),
            rusqlite::params![
                query.dsp,
                query.from,
                query.actor,
                search,
                query.within,
                query.subject,
                query.named,
                query.area
            ],
        )?;
        let mut counts = serde_json::Map::new();
        let mut failures = 0;
        for row in self.platform.all(
            &format!(
                "SELECT {areas} area,count(*) count,sum({FAILED}) failures {filters} GROUP BY 1"
            ),
            rusqlite::params![
                query.dsp,
                query.from,
                query.actor,
                search,
                query.within,
                query.subject,
                query.named
            ],
        )? {
            counts.insert(s(&row, "area").to_owned(), json!(n(&row, "count")));
            failures += n(&row, "failures");
        }
        counts.insert("failures".into(), json!(failures));
        let actors = self.platform.all(
            &format!("SELECT DISTINCT {actor} id,{name} name {FROM} ORDER BY 2"),
            rusqlite::params![query.dsp],
        )?;
        // The platform's log spans every DSP, so it can be narrowed to one.
        let dsps = if query.dsp.is_none() {
            self.platform.all("SELECT DISTINCT d.id,d.name FROM audit a JOIN dsps d ON d.id=a.dsp_id ORDER BY d.name", [])?
        } else {
            Vec::new()
        };
        Ok(serde_json::from_value(
            json!({"events":events,"total":total,"counts":counts,"actors":actors,"dsps":dsps}),
        )?)
    }
}
/// Core's areas, each with the SQL condition on `a.action` its own actions meet and what
/// it answers, in the order the log checks them. `?1` is the DSP whose log it is.
const AREAS: &[(&str, &str)] = &[
    (
        "a.action LIKE 'member.%' OR a.action LIKE 'invitation.%'",
        "'team'",
    ),
    ("a.action LIKE 'role.%'", "'roles'"),
    ("a.action LIKE 'collection.%'", "'collections'"),
    ("a.action LIKE 'schedule.%'", "'schedules'"),
    ("a.action LIKE 'connection.%'", "'connections'"),
    (
        "a.action IN ('dsp.view_opened','dsp.owner_view_opened')",
        "CASE WHEN ?1 IS NULL THEN 'access' ELSE 'team' END",
    ),
    ("a.action LIKE 'account.%'", "'access'"),
    (
        "a.action IN ('dsp.created','dsp.removed','dsp.restored','dsp.suspended','dsp.resumed',\
         'dsp.feature_enabled','dsp.feature_disabled','dsp.feature_hidden','dsp.feature_shown')",
        "'dsps'",
    ),
];
/// The area of the log an event is listed and counted under, as SQL: core's areas, each
/// joined by the prefixes the agents' piece and the features declare for it, and settings for
/// everything else.
fn areas() -> &'static str {
    static AREA: LazyLock<String> = LazyLock::new(|| {
        let agents = registry().agents.into_iter().flat_map(|a| a.audit.areas);
        let declared: Vec<_> = agents
            .chain(registry().features.iter().flat_map(|f| f.audit.areas))
            .collect();
        let mut sql = "CASE".to_owned();
        for (condition, then) in AREAS {
            sql.push_str(&format!(" WHEN {condition}"));
            for (prefix, area) in &declared {
                if *then == format!("'{}'", area.as_str()) {
                    sql.push_str(&format!(" OR a.action LIKE '{prefix}%'"));
                }
            }
            sql.push_str(&format!(" THEN {then}"));
        }
        sql + " ELSE 'settings' END"
    });
    &AREA
}
// A changed field with its previous and new value; either side may be absent.
pub type AuditChange = (&'static str, Option<String>, Option<String>);
pub struct AuditQuery<'a> {
    pub dsp: Option<&'a str>,
    pub area: &'a str,
    pub actor: &'a str,
    pub q: &'a str,
    pub from: &'a str,
    pub before: i64,
    pub limit: i64,
    // One DSP within the platform's log.
    pub within: &'a str,
    // Events about one subject: its "kind:id" reference, or the name older events kept.
    pub subject: &'a str,
    pub named: &'a str,
}
impl Default for AuditQuery<'_> {
    fn default() -> Self {
        Self {
            dsp: None,
            area: "",
            actor: "",
            q: "",
            from: "",
            before: 0,
            limit: 50,
            within: "",
            subject: "",
            named: "",
        }
    }
}
