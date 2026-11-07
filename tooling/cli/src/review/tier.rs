//! How deep a change's review goes, from the paths it changes: none for documentation and tests
//! alone, one pass for the rest, and a security review beside each pass for what a missed bug
//! costs most in.
use std::fmt;

/// Ordered from the least review to the most.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Documentation and tests alone: no review.
    Light,
    /// One pass, and another only for what fixing an Important finding changed.
    Standard,
    /// Each pass with a security review beside it.
    Sensitive,
}
impl Tier {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "light" => Some(Self::Light),
            "standard" => Some(Self::Standard),
            "sensitive" => Some(Self::Sensitive),
            _ => None,
        }
    }
}
impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Light => "light",
            Self::Standard => "standard",
            Self::Sensitive => "sensitive",
        })
    }
}

/// A changed file and its added and deleted lines, as `git diff --numstat` counts them.
pub struct Changed {
    pub path: String,
    pub lines: usize,
}
/// A change's tier and why.
pub struct Judged {
    pub tier: Tier,
    pub reasons: Vec<String>,
}

/// Changed lines past which a change is Sensitive for its size alone.
pub const LARGE: usize = 1000;

/// What makes a path Sensitive, if anything.
pub fn sensitive(path: &str) -> Option<&'static str> {
    let under = |prefix: &str| path.starts_with(prefix);
    let segment = |name: &str| path.split('/').any(|part| part == name);
    if segment("migrations") {
        Some("migrations")
    } else if under("core/accounts/") {
        Some("accounts, sessions and MFA")
    } else if matches!(
        path,
        "core/tenancy/backend/roles.rs" | "core/manifest/backend/permissions.rs"
    ) {
        Some("roles and permissions")
    } else if path.ends_with("/api/routes.rs") || path.contains("/api/routes/") {
        Some("route access")
    } else if under(".github/") || under("tooling/ci/") {
        Some("CI")
    } else if under("ops/host-manager/")
        || under("ops/launchers/")
        || under("ops/scripts/")
        || under("ops/systemd/")
        || path == "core/server/backend/operations/backup.rs"
    {
        Some("the host manager, releases, updates and backups")
    } else if matches!(
        path,
        "mcp/backend/gate.rs" | "mcp/backend/keys.rs" | "mcp/backend/token.rs"
    ) || under("mcp/backend/oauth/")
        || path == "mcp/backend/toolbox/grants.rs"
        || path == "mcp/backend/toolbox/cx.rs"
    {
        Some("MCP access")
    } else if (under("collectors/") && path.contains("/connection/"))
        || under("core/collection/backend/browser/")
        || path == "core/collection/backend/connections.rs"
        || path == "core/collection/backend/verification.rs"
    {
        Some("collector credentials")
    } else if under("android/relay/src/main/kotlin/com/dspdispatch/relay/pairing/")
        || under("android/relay/src/main/kotlin/com/dspdispatch/relay/update/")
    {
        Some("Relay's pairing and updates")
    } else {
        None
    }
}

/// Whether a path holds tests alone: a tests folder, a TypeScript test, or a Rust module's
/// `tests.rs`, which its parent holds only under `#[cfg(test)]`.
pub fn test(path: &str) -> bool {
    path.split('/')
        .any(|part| matches!(part, "tests" | "test" | "androidTest" | "__tests__"))
        || path.ends_with(".test.ts")
        || path.ends_with(".test.tsx")
        || path.ends_with("/tests.rs")
}

/// Whether a path is documentation the product doesn't read: Markdown, but not the AI Support
/// worker's prompts and skills under `services/`, nor the review's own instructions.
fn documentation(path: &str) -> bool {
    path.ends_with(".md") && !path.starts_with("services/") && !path.starts_with("tooling/review/")
}

/// Whether a path's lines count towards a change's size: lockfiles and generated files don't.
fn counted(path: &str) -> bool {
    !path.ends_with("package-lock.json")
        && !path.ends_with("Cargo.lock")
        && !path.split('/').any(|part| part == "generated")
}

/// The tier of a change of `changed`, with each reason it is Sensitive.
pub fn judge(changed: &[Changed]) -> Judged {
    let mut reasons: Vec<String> = vec![];
    for file in changed {
        if let Some(reason) = sensitive(&file.path)
            && !reasons.iter().any(|known| known == reason)
        {
            reasons.push(reason.to_owned());
        }
    }
    let lines: usize = changed
        .iter()
        .filter(|file| counted(&file.path))
        .map(|file| file.lines)
        .sum();
    if lines >= LARGE {
        reasons.push(format!("{lines} changed lines"));
    }
    let tier = if !reasons.is_empty() {
        Tier::Sensitive
    } else if changed
        .iter()
        .all(|file| documentation(&file.path) || test(&file.path))
    {
        Tier::Light
    } else {
        Tier::Standard
    };
    Judged { tier, reasons }
}

/// Reads `git diff --numstat --no-renames -z` into changed files, each path as it is; a
/// binary file counts no lines.
pub fn numstat(text: &str) -> Vec<Changed> {
    text.split('\0')
        .filter_map(|entry| {
            let mut fields = entry.trim_start_matches('\n').splitn(3, '\t');
            let added = fields.next()?;
            let deleted = fields.next()?;
            let path = fields.next().filter(|path| !path.is_empty())?.to_owned();
            let lines = added.parse::<usize>().unwrap_or(0) + deleted.parse::<usize>().unwrap_or(0);
            Some(Changed { path, lines })
        })
        .collect()
}
