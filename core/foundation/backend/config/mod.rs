use crate::{Result, ensure, text_enum};
use std::{env, path::PathBuf};
mod security;
pub use security::SecurityPolicy;
#[derive(Clone)]
pub struct Config {
    pub security: SecurityPolicy,
    pub root: PathBuf,
    pub environment: Environment,
    pub development: bool,
    pub trusted_proxy: crate::server::http::proxy::TrustedProxy,
    pub standalone: bool,
    /// The platform owner's address: the dashboard's admin, the agent API and its sign-in.
    pub origin: String,
    /// Where a new DSP's owner accepts their invitation and sets the DSP up.
    pub invite_origin: String,
    /// Each DSP's address, its short code in place of `{code}` in the first label.
    pub dsp_origin: String,
    pub port: u16,
    pub release: String,
    /// The source this runtime was built from: its commit, and its version when it is a published
    /// release. A server running from a checkout rather than a build knows neither.
    pub source: crate::accounts::api::types::RuntimeSource,
    pub fixture: bool,
    pub fixture_url: Option<String>,
    /// Whether passwords are tried without a limit, as on a preview, whose checkers sign in
    /// again and again: only with fixture providers, while developing.
    pub unlimited_sign_in: bool,
    /// Whether a test may move the live readers' clock on, rather than wait for each read's
    /// slot (`collection::readers::move_clock`): only with fixture providers, while developing.
    pub movable_clock: bool,
    pub browser_capacity: usize,
    pub dashboard: PathBuf,
    pub browseros: PathBuf,
    pub sandbox: PathBuf,
    pub mail_mode: String,
    pub mail_worker_url: Option<String>,
    pub mail_worker_token: Option<String>,
    pub smtp_url: Option<String>,
    pub mail_from: Option<String>,
    /// The bucket the DSPs' pictures are kept in (`db::objects`): none keeps them on this
    /// server's disk alone.
    pub objects: Option<ObjectStore>,
}
/// A bucket of Cloudflare R2, spoken to as S3 is, and the key that may use it. Its secret is
/// never shown: it has no `Debug`.
#[derive(Clone)]
pub struct ObjectStore {
    /// Where it answers: the account's R2 address.
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Whether the bytes it keeps may leave the server's disk, to be read back from it as
    /// they're needed (`R2_ONLY`, `1`): off, they're copied to it and stay on disk too, so a
    /// release that reads only the disk still finds them. Turned on once the release a rollback
    /// would return to reads the object store.
    pub only: bool,
}
fn variable(name: &str, fallback: &str) -> String {
    env::var(name).unwrap_or_else(|_| fallback.into())
}
impl Config {
    pub fn load() -> Result<Self> {
        let bundle = env::var_os("DISPATCH_ARTIFACT_ROOT")
            .map(PathBuf::from)
            .unwrap_or(env::current_dir()?);
        let environment = Environment::parse(&variable("DISPATCH_ENVIRONMENT", "preview"))
            .ok_or_else(|| crate::Error::new("invalid_environment", 400))?;
        let mode = variable(
            "NODE_ENV",
            if environment.is_production() {
                "production"
            } else {
                "development"
            },
        );
        ensure(
            ["development", "test", "production"].contains(&mode.as_str()),
            "invalid_runtime_mode",
            400,
        )?;
        let development = mode != "production";
        ensure(
            !environment.is_production() || !development,
            "production_configuration_required",
            400,
        )?;
        if environment.is_production() {
            ensure(
                [
                    "DISPATCH_STATE_ROOT",
                    "DISPATCH_ORIGIN",
                    "DISPATCH_INVITE_ORIGIN",
                    "DISPATCH_DSP_ORIGIN",
                ]
                .iter()
                .all(|key| env::var(key).is_ok_and(|v| !v.trim().is_empty())),
                "production_configuration_required",
                400,
            )?;
        }
        let provider = variable("DISPATCH_PROVIDER_MODE", "fixture");
        ensure(
            ["native", "fixture"].contains(&provider.as_str()),
            "invalid_provider_mode",
            400,
        )?;
        let mail_prefix = environment.variable_prefix();
        let mail_variable = |suffix: &str| {
            env::var(format!("{mail_prefix}_{suffix}"))
                .ok()
                .filter(|v| !v.trim().is_empty())
        };
        let source = source(&bundle, environment.is_production());
        let mut c = Self {
            security: SecurityPolicy::parse(&variable("DISPATCH_SECURITY_POLICY", "{}"))?,
            root: PathBuf::from(variable(
                "DISPATCH_STATE_ROOT",
                "/tmp/dispatch-rust-development",
            )),
            environment,
            development,
            trusted_proxy: crate::server::http::proxy::TrustedProxy::parse(&variable(
                "DISPATCH_TRUSTED_PROXY",
                "none",
            ))?,
            standalone: variable("DISPATCH_STANDALONE", "1") == "1",
            origin: variable("DISPATCH_ORIGIN", "http://127.0.0.1:5173"),
            invite_origin: String::new(),
            dsp_origin: String::new(),
            port: variable("PORT", "5180")
                .parse()
                .map_err(|_| crate::Error::new("invalid_port", 400))?,
            release: variable("DISPATCH_RELEASE", "development"),
            source,
            fixture: provider == "fixture",
            fixture_url: env::var("DISPATCH_FIXTURE_PROVIDER_URL").ok(),
            unlimited_sign_in: variable("DISPATCH_FIXTURE_UNLIMITED_SIGN_IN", "0") == "1",
            movable_clock: variable("DISPATCH_FIXTURE_MOVABLE_CLOCK", "0") == "1",
            browser_capacity: 2,
            dashboard: bundle.join("dashboard"),
            browseros: PathBuf::from(variable(
                "DISPATCH_BROWSEROS_EXECUTABLE",
                "/opt/dispatch-browseros/0.50.5/browseros",
            )),
            sandbox: PathBuf::from(variable("DISPATCH_BWRAP_EXECUTABLE", "/usr/bin/bwrap")),
            mail_mode: mail_variable("MAIL_MODE")
                .unwrap_or_else(|| if development { "capture" } else { "disabled" }.into()),
            mail_worker_url: mail_variable("MAIL_WORKER_URL"),
            mail_worker_token: mail_variable("MAIL_WORKER_TOKEN"),
            smtp_url: mail_variable("SMTP_URL"),
            mail_from: mail_variable("MAIL_FROM"),
            objects: object_store(|suffix| {
                let name = format!("{mail_prefix}_{suffix}");
                (name.clone(), mail_variable(suffix))
            })?,
        };
        if bundle.join("release.json").exists() {
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(bundle.join("release.json"))?)?;
            c.release = manifest["digest"].as_str().unwrap_or("unknown").into();
        }
        ensure(
            c.root.is_absolute() && c.root.parent().is_some(),
            "absolute_state_root_required",
            400,
        )?;
        let origin = url::Url::parse(&c.origin)
            .map_err(|_| crate::Error::new("canonical_origin_required", 400))?;
        ensure(
            origin.origin().ascii_serialization() == c.origin
                && origin.username().is_empty()
                && origin.password().is_none(),
            "canonical_origin_required",
            400,
        )?;
        ensure(
            development
                || (origin.scheme() == "https"
                    && (!c.fixture || (c.standalone && c.environment.is_preview()))),
            "production_configuration_required",
            400,
        )?;
        (c.invite_origin, c.dsp_origin) = addresses(
            &origin,
            development,
            env::var("DISPATCH_INVITE_ORIGIN").ok(),
            env::var("DISPATCH_DSP_ORIGIN").ok(),
        )?;
        if let Some(url) = &c.fixture_url {
            let url =
                url::Url::parse(url).map_err(|_| crate::Error::new("fixture_forbidden", 403))?;
            ensure(
                c.development
                    && c.fixture
                    && url.scheme() == "http"
                    && url.host_str() == Some("fixture.dispatch.invalid")
                    && url.port().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.path() == "/"
                    && url.query().is_none()
                    && url.fragment().is_none(),
                "fixture_forbidden",
                403,
            )?;
        }
        // The day a test server takes as today: only with fixture providers, while developing.
        // Loading the config pins it for the process (`clock`).
        let today = env::var("DISPATCH_FIXTURE_TODAY")
            .ok()
            .map(|day| {
                chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d")
                    .map_err(|_| crate::Error::new("invalid_fixture_today", 400))
            })
            .transpose()?;
        ensure(
            today.is_none() || (c.development && c.fixture),
            "fixture_forbidden",
            403,
        )?;
        ensure(
            !c.unlimited_sign_in || (c.development && c.fixture),
            "fixture_forbidden",
            403,
        )?;
        ensure(
            !c.movable_clock || (c.development && c.fixture),
            "fixture_forbidden",
            403,
        )?;
        ensure(
            ["capture", "cloudflare", "smtp", "disabled"].contains(&c.mail_mode.as_str()),
            "invalid_mail_mode",
            400,
        )?;
        ensure(
            c.mail_mode != "capture" || development,
            "mail_capture_requires_development",
            400,
        )?;
        if c.mail_mode == "cloudflare" {
            let endpoint = c
                .mail_worker_url
                .as_deref()
                .and_then(|u| url::Url::parse(u).ok())
                .ok_or_else(|| crate::Error::new("mail_configuration_required", 400))?;
            ensure(
                (endpoint.scheme() == "https"
                    || (development
                        && endpoint.scheme() == "http"
                        && endpoint.host_str() == Some("127.0.0.1")))
                    && endpoint.username().is_empty()
                    && endpoint.password().is_none()
                    && endpoint.query().is_none()
                    && endpoint.fragment().is_none()
                    && c.mail_worker_token.as_ref().is_some_and(|t| t.len() >= 32),
                "mail_configuration_required",
                400,
            )?;
        }
        if c.mail_mode == "smtp" {
            let smtp = c
                .smtp_url
                .as_deref()
                .and_then(|value| url::Url::parse(value).ok());
            ensure(
                smtp.as_ref()
                    .is_some_and(|endpoint| secure_smtp_url(endpoint, c.development))
                    && c.mail_from.is_some(),
                "mail_configuration_required",
                400,
            )?;
        }
        ensure(c.standalone, "independent_environment_required", 400)?;
        crate::foundation::clock::pin(today);
        Ok(c)
    }
    /// Which of this server's addresses a request's `Host` names, if any. This machine's own
    /// address is the admin's, as the updater and health checks call it.
    pub fn site(&self, host: &str) -> Option<Site> {
        let host = host.to_ascii_lowercase();
        if host == authority(&self.origin) || host == format!("127.0.0.1:{}", self.port) {
            return Some(Site::Admin);
        }
        if host == authority(&self.invite_origin) {
            return Some(Site::Invite);
        }
        let suffix = authority(&self.dsp_origin).strip_prefix(CODE)?;
        let code = host.strip_suffix(suffix)?;
        (short_code(code) && !self.reserved_code(code)).then(|| Site::Dsp(code.to_owned()))
    }
    /// The origin a site's pages are served from, which its requests must come from.
    pub fn site_origin(&self, site: &Site) -> String {
        match site {
            Site::Admin => self.origin.clone(),
            Site::Invite => self.invite_origin.clone(),
            Site::Dsp(code) => self.dsp_url(code),
        }
    }
    /// The address of the DSP with short code `code`.
    pub fn dsp_url(&self, code: &str) -> String {
        self.dsp_origin
            .replacen(CODE, &code.to_ascii_lowercase(), 1)
    }
    /// A code no DSP may take: a name kept for the platform's own addresses, or one whose DSP
    /// address would be the admin's or the invite page's.
    pub fn reserved_code(&self, code: &str) -> bool {
        let code = code.to_ascii_lowercase();
        let address = self.dsp_url(&code);
        RESERVED_CODES.contains(&code.as_str())
            || address == self.origin
            || address == self.invite_origin
    }
    pub fn platform(&self) -> PathBuf {
        self.root.join("data/platform")
    }
    pub fn environment_root(&self) -> PathBuf {
        self.root.join("data").join(self.environment.as_str())
    }
    pub fn mail_available(&self) -> bool {
        self.mail_mode != "disabled"
    }
    /// A feature's own setting, as its manifest declares it, for this environment:
    /// `DISPATCH_DEV_<name>` on Dev and its previews, `DISPATCH_PRODUCTION_<name>` on
    /// Production. A blank one is unset.
    pub fn setting(&self, name: &str) -> Option<String> {
        env::var(self.setting_variable(name))
            .ok()
            .filter(|value| !value.trim().is_empty())
    }
    /// The variable a feature's setting `name` is read from in this environment.
    pub fn setting_variable(&self, name: &str) -> String {
        let prefix = self.environment.variable_prefix();
        format!("{prefix}_{name}")
    }
}

/// Which of the server's addresses a request came to: the platform owner's admin, the page
/// a new DSP's owner sets it up on, or a DSP's own, by its short code in lowercase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Site {
    Admin,
    Invite,
    Dsp(String),
}

/// What stands for a DSP's short code in `DISPATCH_DSP_ORIGIN`.
const CODE: &str = "{code}";
/// Names kept for addresses the platform has or may have: never a DSP's short code.
const RESERVED_CODES: &[&str] = &[
    "admin",
    "api",
    "app",
    "assets",
    "auth",
    "blog",
    "cdn",
    "dev",
    "dispatch",
    "dispatchbot",
    "dispatchdev",
    "docs",
    "email",
    "ftp",
    "help",
    "imap",
    "invite",
    "login",
    "mail",
    "pop",
    "smtp",
    "static",
    "staging",
    "status",
    "support",
    "test",
    "www",
];

/// Whether `code` can be a DSP's short code: 2 to 16 letters and digits, as it appears in
/// its address, in lowercase.
pub fn short_code(code: &str) -> bool {
    (2..=16).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// What follows the scheme of an origin: its host, and its port when it names one.
fn authority(origin: &str) -> &str {
    origin.split_once("://").map_or(origin, |(_, rest)| rest)
}

/// The invite page's and the DSPs' addresses: as configured, or in development or beside a
/// loopback origin, the same server under `localhost`'s names, since a browser sends every
/// `*.localhost` to its own machine. A deployed server names both.
fn addresses(
    origin: &url::Url,
    development: bool,
    invite: Option<String>,
    dsp: Option<String>,
) -> Result<(String, String)> {
    let blank = |value: Option<String>| value.filter(|v| !v.trim().is_empty());
    let loopback = match origin.host() {
        Some(url::Host::Domain(host)) => host == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    let local = |label: &str| {
        let port = origin
            .port()
            .map_or(String::new(), |port| format!(":{port}"));
        format!("{}://{label}.localhost{port}", origin.scheme())
    };
    let (invite, dsp) = match (blank(invite), blank(dsp)) {
        (Some(invite), Some(dsp)) => (invite, dsp),
        (None, None) if development || loopback => (local("invite"), local(CODE)),
        _ => return Err(crate::Error::new("address_configuration_required", 400)),
    };
    let canonical = |address: &str| {
        url::Url::parse(address).is_ok_and(|url| {
            url.origin().ascii_serialization() == address
                && url.scheme() == origin.scheme()
                && url.username().is_empty()
                && url.password().is_none()
        })
    };
    let sample = dsp.replacen(CODE, "dsp", 1);
    ensure(
        canonical(&invite)
            && invite != origin.origin().ascii_serialization()
            && authority(&dsp).starts_with(&format!("{CODE}."))
            && !sample.contains(['{', '}'])
            && canonical(&sample),
        "address_configuration_required",
        400,
    )?;
    Ok((invite, dsp))
}

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/foundation/api/generated/"))]
    #[derive(schemars::JsonSchema)]
        pub enum Environment {
        Preview => "preview",
        Production => "production",
    }
}
impl Environment {
    fn variable_prefix(self) -> &'static str {
        match self {
            Self::Preview => "DISPATCH_DEV",
            Self::Production => "DISPATCH_PRODUCTION",
        }
    }
    pub fn is_preview(self) -> bool {
        self == Self::Preview
    }
    pub fn is_production(self) -> bool {
        self == Self::Production
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/foundation/api/generated/"))]
        pub enum ProviderMode {
        Fixture => "fixture",
        Native => "native",
    }
}

/// The object store the environment's `R2_ACCOUNT_ID`, `R2_BUCKET`, `R2_ACCESS_KEY_ID` and
/// `R2_SECRET_ACCESS_KEY` name, and whether bytes may leave the disk for it, `R2_ONLY` (`1`, or
/// `0` as unset), each as `variable` reads it with the variable it's read from: none with none of
/// them set, and refused as `object_store_incomplete` with only some, naming those missing in the
/// log, never a value.
fn object_store(
    variable: impl Fn(&str) -> (String, Option<String>),
) -> Result<Option<ObjectStore>> {
    let read = [
        "R2_ACCOUNT_ID",
        "R2_BUCKET",
        "R2_ACCESS_KEY_ID",
        "R2_SECRET_ACCESS_KEY",
    ]
    .map(&variable);
    let (_, only) = variable("R2_ONLY");
    let missing: Vec<&str> = read
        .iter()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name.as_str())
        .collect();
    if missing.len() == read.len() && only.is_none() {
        return Ok(None);
    }
    if !missing.is_empty() {
        crate::foundation::observability::event(
            "error",
            "object_store_incomplete",
            serde_json::json!({ "missing": missing }),
        );
        return Err(crate::Error::new("object_store_incomplete", 400));
    }
    let [account, bucket, access_key_id, secret_access_key] =
        read.map(|(_, value)| value.unwrap_or_default());
    let named = |value: &str, more: &[u8]| {
        !value.is_empty()
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || more.contains(&b))
    };
    let only = only.as_deref().map(str::trim);
    ensure(
        named(&account, b"") && named(&bucket, b"-") && matches!(only, None | Some("0" | "1")),
        "invalid_object_store",
        400,
    )?;
    Ok(Some(ObjectStore {
        endpoint: format!("https://{account}.r2.cloudflarestorage.com"),
        bucket,
        access_key_id,
        secret_access_key,
        only: only == Some("1"),
    }))
}
fn secure_smtp_url(endpoint: &url::Url, development: bool) -> bool {
    endpoint.host_str().is_some()
        && endpoint.fragment().is_none()
        && (endpoint.scheme() == "smtps"
            || (endpoint.scheme() == "smtp"
                && endpoint
                    .query_pairs()
                    .eq([("tls".into(), "required".into())]))
            || (development
                && endpoint.scheme() == "smtp"
                && endpoint.host_str() == Some("127.0.0.1")
                && endpoint.query().is_none()))
}
/// The commit a build was made from, and on Production its version: Production installs only
/// published releases, whose manifest names it. Any other build still carries package.json's
/// version, which no release has used.
fn source(
    bundle: &std::path::Path,
    production: bool,
) -> crate::accounts::api::types::RuntimeSource {
    let field = |file: &str, key: &str| {
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.join(file)).ok()?).ok()?;
        value[key].as_str().map(String::from)
    };
    crate::accounts::api::types::RuntimeSource {
        version: production
            .then(|| field("release.json", "version"))
            .flatten(),
        commit: field("tooling/build-info.json", "commit"),
    }
}

#[cfg(test)]
#[path = "../../tests/backend/config/mod.rs"]
mod tests;

/// Its settings' kinds, which the app's export writes to TypeScript.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> crate::Typescript {
    crate::typescript!(cfg, Environment, ProviderMode)
}
