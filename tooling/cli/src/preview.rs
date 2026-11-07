//! What a change shows, approved before its PR opens. `preview approve` keeps the UI files the
//! user was shown, by their contents, once they approved them; `pr` opens no PR for a change to
//! what a page shows until those are the files it would push.
use crate::{Result, Runner, require, review, workspace};
use std::{collections::BTreeMap, fs, path::Path};

/// Where an approval is kept, in the checkout's scratch folder.
const APPROVED: &str = "preview/approved.json";

/// A change's UI files against what the user approved.
#[derive(Debug, PartialEq, Eq)]
pub enum Approval {
    /// The change touches nothing a page shows.
    None,
    /// The files are the ones approved.
    Approved,
    /// Nothing was approved yet.
    Missing,
    /// These differ from what was approved.
    Changed(Vec<String>),
}

/// Each UI file the change touches against `origin/main`, by the hash of its contents, or
/// `deleted`: committed, or as the worktree holds it with `worktree`.
fn files(root: &Path, worktree: bool, runner: &dyn Runner) -> Result<BTreeMap<String, String>> {
    let git = |args: &[&str]| {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        workspace::text(runner, &command, Some(root))
    };
    let main = git(&["merge-base", "origin/main", "HEAD"])?;
    let against = match worktree {
        true => main.clone(),
        false => format!("{main}..HEAD"),
    };
    let mut paths: Vec<String> = git(&["diff", "--name-only", "--no-renames", &against])?
        .lines()
        .map(str::to_owned)
        .collect();
    if worktree {
        paths.extend(
            git(&["ls-files", "--others", "--exclude-standard"])?
                .lines()
                .map(str::to_owned),
        );
    }
    let mut files = BTreeMap::new();
    for path in paths.into_iter().filter(|path| review::ui(path)) {
        let hash = match worktree {
            true if root.join(&path).exists() => git(&["hash-object", "--", &path])?,
            false => {
                git(&["rev-parse", "--verify", "-q", &format!("HEAD:{path}")]).unwrap_or_default()
            }
            _ => String::new(),
        };
        let hash = if hash.is_empty() {
            "deleted".to_owned()
        } else {
            hash
        };
        files.insert(path, hash);
    }
    Ok(files)
}

/// The committed change's UI files against what the user approved.
pub fn approval(root: &Path, runner: &dyn Runner) -> Result<Approval> {
    let now = files(root, false, runner)?;
    if now.is_empty() {
        return Ok(Approval::None);
    }
    let path = workspace::checkout_scratch(root)?.join(APPROVED);
    let approved: BTreeMap<String, String> = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)?,
        Err(_) => return Ok(Approval::Missing),
    };
    let changed: Vec<String> = now
        .iter()
        .filter(|(path, hash)| approved.get(*path) != Some(hash))
        .map(|(path, _)| path.clone())
        .collect();
    Ok(match changed.is_empty() {
        true => Approval::Approved,
        false => Approval::Changed(changed),
    })
}

/// `dispatchdev preview approve`: the UI files as the worktree holds them, kept as what the user
/// approved. Run only once they said so.
pub fn approve(root: &Path, runner: &dyn Runner) -> Result<()> {
    let files = files(root, true, runner)?;
    require(
        !files.is_empty(),
        "The change touches nothing a page shows: there is nothing to approve.",
    )?;
    let path = workspace::checkout_scratch(root)?.join(APPROVED);
    workspace::write_private(&path, &serde_json::to_string_pretty(&files)?)?;
    println!(
        "Kept {} UI file{} as approved: {}.",
        files.len(),
        if files.len() == 1 { "" } else { "s" },
        files.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    println!(
        "{}",
        workspace::next(&["Commit them, then `dispatchdev pr` opens the PR".to_owned()])
    );
    Ok(())
}
