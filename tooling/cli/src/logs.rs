//! `dispatchdev logs <name>`: what a change's preview, or Dev, has logged, the server's events a
//! line each and without the build output around them. It reads this machine's journal only;
//! Production's logs hold real data and stay there.
use crate::{Result, Runner, preview, workspace};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

pub struct Options<'a> {
    /// Only warnings, errors and failures.
    pub errors: bool,
    /// From this time, across runs, instead of the current run: `10m`, `2h` or any time
    /// journalctl reads.
    pub since: Option<&'a str>,
    /// Keep printing what it logs next.
    pub follow: bool,
}

/// Cargo's progress, npm's script echo and the build cache's notes, which a preview prints as
/// it builds.
const BUILD: &[&str] = &[
    "Compiling ",
    "Checking ",
    "Finished ",
    "Downloaded ",
    "Downloading ",
    "Updating ",
    "Locking ",
    "Blocking ",
    "Fresh ",
    "Building ",
    "Reused ",
    "Cached ",
    "> ",
];

/// The unit whose journal `name` names: Dev's, or a change's preview.
pub fn unit(name: &str) -> String {
    match name {
        "dev" => "dispatch-dev.service".into(),
        name => format!("{}.service", preview::unit(name)),
    }
}
/// journalctl's `--since` for `since`: a short span such as `10m` counts back from now.
pub fn since(since: &str) -> String {
    let digits = since.bytes().take_while(u8::is_ascii_digit).count();
    let short = digits > 0 && ["s", "m", "min", "h", "d"].contains(&&since[digits..]);
    if short {
        format!("--since=-{since}")
    } else {
        format!("--since={since}")
    }
}
/// A field's value as a line reads it: text as it is, or `-` when empty, numbers to a tenth,
/// anything else as JSON.
fn value(value: &Value) -> String {
    match value {
        Value::String(text) if text.is_empty() => "-".into(),
        Value::String(text) if text.contains(' ') => format!("{text:?}"),
        Value::String(text) => text.clone(),
        Value::Number(number) if number.is_f64() => {
            format!("{:.1}", number.as_f64().unwrap_or_default())
        }
        other => other.to_string(),
    }
}
/// A server event as one line: its time (UTC), level and name, then for a request what was
/// asked and how it ended, or else its fields.
fn event(event: &Value) -> Option<String> {
    let at = event["at"].as_str()?;
    let time = at
        .get(11..19)
        .map_or(at.to_owned(), |time| format!("{time}Z"));
    let level = event["level"].as_str()?;
    let name = event["event"].as_str()?;
    let fields = &event["fields"];
    let detail = if name == "http.request" {
        let mut parts = vec![
            value(&fields["method"]),
            value(&fields["route"]),
            value(&fields["status"]),
        ];
        if let Some(error) = fields["error"].as_str() {
            parts.push(error.to_owned());
        }
        parts.push(format!("{}ms", value(&fields["elapsedMs"])));
        parts.push(value(&fields["requestId"]));
        parts.join(" ")
    } else {
        fields
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, field)| !field.is_null())
            .map(|(key, field)| format!("{key}={}", value(field)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    Some(
        format!("{time}  {level:<5}  {name}  {detail}")
            .trim_end()
            .to_owned(),
    )
}
/// Whether output that isn't an event reads as a failure.
fn failing(line: &str) -> bool {
    let lower = line.to_lowercase();
    ["error", "fail", "panic", "fatal"]
        .iter()
        .any(|sign| lower.contains(sign))
}
/// One logged line as `logs` shows it, or nothing: the build's output never, and with
/// `errors`, only warnings, errors and failures.
pub fn line(raw: &str, errors: bool) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed)
        && let Some(shown) = event(&parsed)
    {
        let level = parsed["level"].as_str().unwrap_or("");
        return (!errors || matches!(level, "warn" | "error")).then_some(shown);
    }
    if errors {
        return failing(trimmed).then(|| raw.trim_end().to_owned());
    }
    (!BUILD.iter().any(|noise| trimmed.starts_with(noise))).then(|| raw.trim_end().to_owned())
}
/// journalctl's arguments for what `options` asks of `name`'s unit, or nothing when it asks for
/// the current run and there is none.
fn query(name: &str, options: &Options, runner: &dyn Runner) -> Result<Option<Vec<String>>> {
    let unit = unit(name);
    let mut args: Vec<String> = ["journalctl", "--user", "-o", "cat", "--no-pager"]
        .map(str::to_owned)
        .to_vec();
    match options.since {
        Some(time) => args.extend(["-u".to_owned(), unit, since(time)]),
        None => {
            let id = workspace::text(
                runner,
                &[
                    "systemctl",
                    "--user",
                    "show",
                    "-p",
                    "InvocationID",
                    "--value",
                    &unit,
                ],
                None,
            )?;
            if id.is_empty() {
                return Ok(None);
            }
            args.push(format!("_SYSTEMD_INVOCATION_ID={id}"));
        }
    }
    if options.follow {
        args.extend(["--follow", "--no-tail"].map(str::to_owned));
    }
    Ok(Some(args))
}
pub fn run(name: &str, options: &Options, runner: &dyn Runner) -> Result<()> {
    if name != "dev" {
        workspace::valid_name(name)?;
    }
    let what = match name {
        "dev" => "Dev".to_owned(),
        name => format!("{name}'s preview"),
    };
    let Some(args) = query(name, options, runner)? else {
        println!("{what} isn't running. --since <time> shows its earlier runs.");
        return Ok(());
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // A reader that stops early, as head does, ends the listing.
    let mut show = |text: &str| writeln!(out, "{text}").is_ok();
    if options.follow {
        let mut child = Command::new(args[0])
            .args(&args[1..])
            .stdout(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()?;
        let journal = child.stdout.take().ok_or("journalctl gave no output")?;
        for raw in BufReader::new(journal).lines() {
            if let Some(text) = line(&raw?, options.errors)
                && !show(&text)
            {
                break;
            }
        }
        let _ = child.kill();
        child.wait()?;
        return Ok(());
    }
    let log = workspace::text(runner, &args, None)?;
    let mut shown = 0;
    for text in log.lines().filter_map(|raw| line(raw, options.errors)) {
        if !show(&text) {
            return Ok(());
        }
        shown += 1;
    }
    if shown == 0 {
        let window = match options.since {
            Some(time) => format!("since {time}"),
            None => "in its current run".to_owned(),
        };
        let kind = if options.errors {
            "no warnings or errors"
        } else {
            "nothing"
        };
        println!("{what} logged {kind} {window}.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_name_is_a_preview_or_dev() {
        assert_eq!(unit("dev"), "dispatch-dev.service");
        assert_eq!(
            unit("audit-timeline"),
            format!("{}.service", preview::unit("audit-timeline"))
        );
    }
    #[test]
    fn a_short_span_counts_back_from_now() {
        assert_eq!(since("10m"), "--since=-10m");
        assert_eq!(since("2h"), "--since=-2h");
        assert_eq!(since("90s"), "--since=-90s");
        assert_eq!(since("today"), "--since=today");
        assert_eq!(since("2026-10-05 14:00"), "--since=2026-10-05 14:00");
        assert_eq!(since("10 min ago"), "--since=10 min ago");
    }
    #[test]
    fn events_read_as_a_line_each_and_the_build_goes() {
        let request = r#"{"at":"2026-10-05T01:48:41.477Z","level":"warn","event":"http.request","fields":{"requestId":"req_5080","method":"GET","route":"/api/dsp/driver-match","actorId":"usr_1","dspId":null,"account":"a","client":"c","bulk":false,"status":403,"elapsedMs":1,"error":"dsp_view_required"}}"#;
        assert_eq!(
            line(request, false).unwrap(),
            "01:48:41Z  warn   http.request  GET /api/dsp/driver-match 403 dsp_view_required 1ms req_5080"
        );
        let slow = r#"{"at":"2026-10-05T16:53:15.865Z","level":"info","event":"database.slow","fields":{"write":true,"queueMs":0.004818,"totalMs":36.248993999999996,"cause":null,"note":"two words"}}"#;
        assert_eq!(
            line(slow, false).unwrap(),
            r#"16:53:15Z  info   database.slow  write=true queueMs=0.0 totalMs=36.2 note="two words""#
        );
        for build in [
            "   Compiling serde v1.0.229",
            "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.60s",
            "> dispatch-platform@0.0.19 dev",
            "Cached debug backend for identical inputs.",
            "",
        ] {
            assert_eq!(line(build, false), None, "{build}");
        }
        let link = "Development fixtures: http://preview.test:4101/__preview/x";
        assert_eq!(line(link, false).as_deref(), Some(link));
        let unrouted = r#"{"at":"2026-10-05T01:38:03.367Z","level":"warn","event":"http.request","fields":{"requestId":"req_e012","method":"GET","route":"","status":404,"elapsedMs":0,"error":"not_found"}}"#;
        assert_eq!(
            line(unrouted, true).unwrap(),
            "01:38:03Z  warn   http.request  GET - 404 not_found 0ms req_e012"
        );
    }
    #[test]
    fn errors_are_warnings_errors_and_failures() {
        let event = |level: &str| {
            format!(
                r#"{{"at":"2026-10-05T01:00:00.000Z","level":"{level}","event":"x.y","fields":{{}}}}"#
            )
        };
        assert_eq!(line(&event("info"), true), None);
        assert_eq!(
            line(&event("warn"), true).as_deref(),
            Some("01:00:00Z  warn   x.y")
        );
        assert!(line(&event("error"), true).is_some());
        // A unit's name, built as the journal prints it.
        let preview = unit("a");
        for failure in [
            "thread 'main' panicked at core/server/backend/state.rs:10:5:".to_owned(),
            "error[E0308]: mismatched types".to_owned(),
            format!("{preview}: Failed with result 'exit-code'."),
        ] {
            assert_eq!(line(&failure, true), Some(failure));
        }
        assert_eq!(line(&format!("Started {preview}"), true), None);
        assert_eq!(line("   Compiling serde v1.0.229", true), None);
    }
}
