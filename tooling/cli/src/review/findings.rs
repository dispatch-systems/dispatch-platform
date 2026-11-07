//! A pass's findings as its reviewer writes them, a heading each, and the author's answer to
//! every one: what was done about it and, where nothing was fixed, why.
use crate::{Result, require};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tag {
    Important,
    Nit,
    PreExisting,
    Question,
    OutOfScope,
}
impl Tag {
    const ALL: [Self; 5] = [
        Self::Important,
        Self::Nit,
        Self::PreExisting,
        Self::Question,
        Self::OutOfScope,
    ];
    fn heading(self) -> &'static str {
        match self {
            Self::Important => "Important",
            Self::Nit => "Nit",
            Self::PreExisting => "Pre-existing",
            Self::Question => "Question",
            Self::OutOfScope => "Out of scope",
        }
    }
    /// The answers a finding of this tag takes, and whether each needs a reason.
    fn answers(self) -> &'static [(&'static str, bool)] {
        match self {
            Self::Important => &[("fixed", false), ("wrong", true), ("intentional", true)],
            Self::Nit => &[("fixed", false), ("declined", true)],
            Self::PreExisting | Self::OutOfScope => &[("noted", false)],
            Self::Question => &[("changed", false), ("kept", true)],
        }
    }
}
impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.heading())
    }
}

/// Nits a pass reports at most: the rest go unanswered.
pub const NITS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub tag: Tag,
    /// The heading after its tag: where, and what.
    pub what: String,
}
impl Finding {
    fn heading(&self) -> String {
        format!("## {}: {}", self.tag, self.what)
    }
}

/// The findings in a reviewer's file, in order, and how many Nits past the cap it dropped.
pub fn parse(text: &str) -> Result<(Vec<Finding>, usize)> {
    if text.trim() == "No findings." {
        return Ok((vec![], 0));
    }
    let mut findings = vec![];
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("## ") else {
            continue;
        };
        let (tag, what) = rest
            .split_once(':')
            .and_then(|(name, what)| {
                let tag = Tag::ALL
                    .into_iter()
                    .find(|tag| tag.heading() == name.trim())?;
                Some((tag, what.trim()))
            })
            .ok_or_else(|| {
                format!(
                    "\"{line}\" isn't a finding: each heading is ## <{}>: <path>:<line> <what>.",
                    Tag::ALL.map(Tag::heading).join(" | ")
                )
            })?;
        require(
            !what.is_empty(),
            &format!("\"{line}\" says nothing after its tag."),
        )?;
        findings.push(Finding {
            tag,
            what: what.to_owned(),
        });
    }
    require(
        !findings.is_empty(),
        "The findings hold no ## heading and don't say `No findings.` alone.",
    )?;
    let (before, mut nits) = (findings.len(), 0);
    findings.retain(|finding| {
        let keep = finding.tag != Tag::Nit || nits < NITS;
        nits += usize::from(finding.tag == Tag::Nit);
        keep
    });
    let dropped = before - findings.len();
    Ok((findings, dropped))
}

/// The file the author answers in: each finding's heading with an `Answer:` line to fill.
pub fn skeleton(findings: &[Finding]) -> String {
    let mut text = String::from(
        "One answer under each finding: `Answer: <answer>`, then why where it asks for a reason.\n\
         Important: fixed | wrong <why> | intentional <why>. Nit: fixed | declined <why>.\n\
         Question: changed | kept <why>. Pre-existing and Out of scope: noted.\n",
    );
    for finding in findings {
        text.push_str(&format!("\n{}\nAnswer:\n", finding.heading()));
    }
    text
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answered {
    pub finding: Finding,
    pub answer: &'static str,
    pub reason: String,
}

/// Every finding with its answer from `text`, refusing a finding unanswered or answered in a
/// way its tag doesn't take.
pub fn answers(findings: &[Finding], text: &str) -> Result<Vec<Answered>> {
    let lines: Vec<&str> = text.lines().collect();
    findings
        .iter()
        .map(|finding| {
            let heading = finding.heading();
            let at = lines
                .iter()
                .position(|line| line.trim_end() == heading)
                .ok_or_else(|| format!("No answer to \"{heading}\"."))?;
            let given = lines[at + 1..]
                .iter()
                .take_while(|line| !line.starts_with("## "))
                .find_map(|line| line.trim().strip_prefix("Answer:"))
                .map(str::trim)
                .unwrap_or_default();
            require(!given.is_empty(), &format!("No answer to \"{heading}\"."))?;
            let (word, reason) = given
                .split_once(char::is_whitespace)
                .map(|(word, reason)| (word, reason.trim_start_matches(['—', '-', ':', ' '])))
                .unwrap_or((given, ""));
            let &(answer, needs) = finding
                .tag
                .answers()
                .iter()
                .find(|(answer, _)| *answer == word)
                .ok_or_else(|| {
                    format!(
                        "\"{heading}\" is answered \"{given}\": its answer is one of {}.",
                        finding
                            .tag
                            .answers()
                            .iter()
                            .map(|(answer, _)| *answer)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?;
            require(
                !needs || !reason.trim().is_empty(),
                &format!("\"{heading}\" is {answer}: say why after it."),
            )?;
            Ok(Answered {
                finding: finding.clone(),
                answer,
                reason: reason.trim().to_owned(),
            })
        })
        .collect()
}
