//! How the CLI calls GitHub: every `gh` call goes through here. Each try is bounded, so none
//! hangs silently, and a call that failed the way a moment's trouble on the network or at
//! GitHub fails it is tried again, three tries over half a minute, with a line on stderr saying
//! so. A refusal is never tried again, and a call that changes something once is tried again
//! only after GitHub says the failed try didn't make the change.
use crate::{Result, Runner, workspace};
use std::{fmt, path::Path, time::Duration};

/// How long one try may take before it is taken to have hung, and a slower call's: a job's
/// whole log, a merge or a new PR, which GitHub does more work for.
const CALL: u64 = 60;
const SLOW: u64 = 180;
/// The wait before each try after the first.
const BACKOFF: [Duration; 2] = [Duration::from_secs(10), Duration::from_secs(20)];
/// What a failure that passes says, lowercased, where GitHub gave no HTTP status: the network
/// failing on the way, and a try that ran out of time.
const PASSING: [&str; 13] = [
    "timed out",
    "timeout",
    "deadline exceeded",
    "dial tcp",
    "connection reset",
    "connection refused",
    "tls handshake",
    "unexpected eof",
    "error connecting to",
    "no such host",
    "network is unreachable",
    "temporary failure in name resolution",
    "something went wrong while executing your query",
];

/// GitHub didn't answer on any try, for reasons that pass: what waits on it can go on waiting.
#[derive(Debug)]
pub struct Unreachable(String);
impl fmt::Display for Unreachable {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for Unreachable {}

/// What `gh` with `args` prints in `root`, trimmed, for a call that only reads, or whose effect
/// is the same however often it is made, such as setting a PR's title and body: tried again as
/// it is.
pub fn call(runner: &dyn Runner, root: &Path, args: &[&str]) -> Result<String> {
    attempt(runner, root, args, CALL, None)
}
/// A read that takes longer than most: a job's whole log.
pub fn download(runner: &dyn Runner, root: &Path, args: &[&str]) -> Result<String> {
    attempt(runner, root, args, SLOW, None)
}
/// A call that changes something once, such as a merge or a new PR, which GitHub may have made
/// on a try whose answer was lost, and refuses to make twice. After each try that failed for a
/// passing reason, `done` asks GitHub whether the change was made, and answers what the call
/// would have: it is tried again only when that says it wasn't.
pub fn once(
    runner: &dyn Runner,
    root: &Path,
    args: &[&str],
    done: &dyn Fn() -> Result<Option<String>>,
) -> Result<String> {
    attempt(runner, root, args, SLOW, Some(done))
}

fn attempt(
    runner: &dyn Runner,
    root: &Path,
    args: &[&str],
    timeout: u64,
    done: Option<&dyn Fn() -> Result<Option<String>>>,
) -> Result<String> {
    let command = [&["gh"][..], args].concat();
    let name = command[..command.len().min(3)].join(" ");
    let mut waits = BACKOFF.iter().enumerate();
    loop {
        let error = match runner.command(&command, Some(root), timeout) {
            Ok(output) => return Ok(String::from_utf8(output)?.trim().to_owned()),
            Err(error) => error,
        };
        let message = error.to_string();
        if !passing(&message) {
            return Err(error);
        }
        if let Some(done) = done
            && let Some(answer) = done()?
        {
            return Ok(answer);
        }
        let reason = reason(&message, &name);
        let Some((tried, wait)) = waits.next() else {
            return Err(Unreachable(format!(
                "`{name}` failed on all {} tries: {reason}",
                BACKOFF.len() + 1
            ))
            .into());
        };
        eprintln!(
            "`{name}` failed ({reason}): trying again in {}, try {} of {}.",
            workspace::duration(*wait),
            tried + 2,
            BACKOFF.len() + 1
        );
        runner.pause(*wait);
    }
}
/// Whether a failed call failed the way a moment's trouble does: on the network, out of time,
/// or with GitHub answering that it failed itself (5xx), is overloaded (429) or gave up on the
/// request (499). Any other answer of GitHub's, such as a 404, a merge conflict or a check that
/// failed, is a refusal.
fn passing(message: &str) -> bool {
    let message = message.to_lowercase();
    match status(&message) {
        Some(status) => status >= 500 || status == 429 || status == 499,
        None => PASSING.iter().any(|sign| message.contains(sign)),
    }
}
/// The HTTP status `gh` names in a lowercased message, as in `HTTP 502: Bad Gateway`.
fn status(message: &str) -> Option<u16> {
    message.match_indices("http ").find_map(|(at, _)| {
        let code = message.get(at + 5..at + 8)?;
        code.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| code.parse().ok())?
    })
}
/// What a failure of the call `name` says, on one line, without the name the runner puts
/// first.
fn reason(message: &str, name: &str) -> String {
    let message = message
        .strip_prefix(&format!("{name} failed: "))
        .unwrap_or(message);
    let line = message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    match line.char_indices().nth(200) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// GitHub answering each call in turn from `answers`: an output, or the error the runner
    /// reports.
    struct Github {
        answers: RefCell<Vec<std::result::Result<&'static str, &'static str>>>,
        timeouts: RefCell<Vec<u64>>,
        paused: RefCell<Vec<Duration>>,
    }
    impl Github {
        fn new(answers: &[std::result::Result<&'static str, &'static str>]) -> Self {
            Self {
                answers: RefCell::new(answers.iter().rev().copied().collect()),
                timeouts: RefCell::new(vec![]),
                paused: RefCell::new(vec![]),
            }
        }
    }
    impl Runner for Github {
        fn command(&self, args: &[&str], _: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
            assert_eq!(args[..2], ["gh", "run"]);
            self.timeouts.borrow_mut().push(timeout);
            match self.answers.borrow_mut().pop().expect("no more calls") {
                Ok(output) => Ok(output.as_bytes().to_vec()),
                Err(error) => Err(error.into()),
            }
        }
        fn pause(&self, duration: Duration) {
            self.paused.borrow_mut().push(duration);
        }
    }
    fn list(github: &Github) -> std::result::Result<String, String> {
        call(github, Path::new("."), &["run", "list"]).map_err(|error| error.to_string())
    }

    #[test]
    fn a_call_that_failed_for_a_passing_reason_is_tried_again_after_a_wait() {
        // As the runner and `gh` report them: a try cut off at its bound, and the network or
        // GitHub failing.
        for failure in [
            "gh timed out or exceeded output limit",
            "gh run list failed: Get \"https://api.github.com/graphql\": dial tcp: lookup \
             api.github.com: i/o timeout",
            "gh run list failed: error connecting to api.github.com\ncheck your internet \
             connection or https://githubstatus.com",
            "gh run list failed: read tcp: read: connection reset by peer",
            "gh run list failed: net/http: TLS handshake timeout",
            "gh run list failed: HTTP 502: Bad Gateway (https://api.github.com/graphql)",
            "gh run list failed: HTTP 429: too many requests",
            "gh run list failed: HTTP 499",
        ] {
            let github = Github::new(&[Err(failure), Ok("[]\n")]);
            assert_eq!(list(&github), Ok("[]".to_owned()), "{failure}");
            assert_eq!(
                *github.paused.borrow(),
                [Duration::from_secs(10)],
                "{failure}"
            );
            assert_eq!(
                *github.timeouts.borrow(),
                [CALL, CALL],
                "each try is bounded"
            );
        }
        // Three tries over half a minute, then GitHub is unreachable: an error a wait can
        // tell from a refusal.
        let down = "gh run list failed: HTTP 503: Service Unavailable";
        let github = Github::new(&[Err(down), Err(down), Err(down)]);
        let error = call(&github, Path::new("."), &["run", "list"]).unwrap_err();
        assert!(error.is::<Unreachable>(), "{error}");
        assert_eq!(
            error.to_string(),
            "`gh run list` failed on all 3 tries: HTTP 503: Service Unavailable"
        );
        assert_eq!(
            *github.paused.borrow(),
            [Duration::from_secs(10), Duration::from_secs(20)]
        );
    }
    #[test]
    fn a_refusal_is_never_tried_again() {
        for refusal in [
            "gh run list failed: HTTP 404: Not Found (https://api.github.com/repos/o/r/pulls/9)",
            "gh run list failed: HTTP 422: Validation Failed",
            "gh run list failed: HTTP 403: Resource not accessible by integration; \
             connection reset",
            "gh run list failed: Pull request #7 is not mergeable: the merge commit cannot be \
             cleanly created.",
            "gh run list failed: no pull requests found for branch \"change\"",
            "Cannot run gh: No such file or directory (os error 2)",
        ] {
            let github = Github::new(&[Err(refusal)]);
            let error = call(&github, Path::new("."), &["run", "list"]).unwrap_err();
            assert!(!error.is::<Unreachable>(), "{refusal}");
            assert_eq!(error.to_string(), refusal);
            assert!(github.paused.borrow().is_empty(), "{refusal}");
        }
    }
}
