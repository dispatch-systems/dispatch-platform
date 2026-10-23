//! `dispatchdev shot <name>`: a PNG of a page of the change's preview, kept in its scratch
//! folder. The account is signed in as `dispatchdev api` signs it in, once a run of the
//! preview, and the checkout's own `tooling/dev/shot.ts` opens the page with that session in
//! the browser tests' Chromium.
use crate::{
    Result, Runner,
    api::{self, OWNER},
    change::Change,
    preview::{self, Open},
    require,
    workspace::{self, Workspace},
};
use serde_json::{Value, json};

const SCRIPT: &str = "tooling/dev/shot.ts";

pub struct Shot<'a> {
    pub open: Open<'a>,
    /// The account, the owner unless named.
    pub who: Option<&'a str>,
    /// The window's size, `<width>x<height>` in pixels.
    pub size: &'a str,
    /// `light` or `dark`.
    pub theme: &'a str,
}

/// The cookies of a jar curl keeps, as Playwright loads a browser's: a line each, with
/// `#HttpOnly_` before the domain of one a page's scripts can't read.
fn cookies(jar: &str) -> Vec<Value> {
    jar.lines()
        .filter_map(|line| {
            let (line, http_only) = match line.strip_prefix("#HttpOnly_") {
                Some(line) => (line, true),
                None => (line, false),
            };
            let [domain, _, path, secure, expires, name, value] =
                line.split('\t').collect::<Vec<_>>()[..]
            else {
                return None;
            };
            Some(json!({
                "name": name,
                "value": value,
                "domain": domain,
                "path": path,
                // curl writes 0 for a cookie that ends with the browser's session, Playwright -1.
                "expires": expires.parse::<i64>().ok().filter(|at| *at > 0).unwrap_or(-1),
                "httpOnly": http_only,
                "secure": secure == "TRUE",
                // The backend sets every cookie SameSite=Strict, which curl doesn't keep.
                "sameSite": "Strict",
            }))
        })
        .collect()
}

/// The PNG's name, from what it shows: `dvic-northline-member-390x844-dark.png`.
fn file_name(shot: &Shot<'_>) -> String {
    let open = &shot.open;
    let words = [
        Some(open.page.unwrap_or("home")),
        open.tab,
        open.dsp,
        open.platform.then_some("platform"),
        shot.who.and_then(|email| email.split('@').next()),
        Some(shot.size),
        Some(shot.theme),
    ];
    let name = words
        .into_iter()
        .flatten()
        .flat_map(|word| word.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("{}.png", name.to_lowercase())
}

pub fn run(ws: &Workspace, name: &str, shot: &Shot<'_>, runner: &dyn Runner) -> Result<()> {
    let change = Change::existing(ws, name)?;
    let open = &shot.open;
    open.check()?;
    let who = shot.who.unwrap_or(OWNER);
    require(
        !open.platform || who == OWNER,
        "--platform opens the platform owner's own pages: leave out --as.",
    )?;
    let fits = |side: &str| {
        side.parse::<u32>()
            .is_ok_and(|px| (200..=4000).contains(&px))
    };
    require(
        shot.size
            .split_once('x')
            .is_some_and(|(width, height)| fits(width) && fits(height)),
        "--size is the window's width and height in pixels, such as 390x844.",
    )?;
    require(
        matches!(shot.theme, "light" | "dark"),
        "--theme is light or dark.",
    )?;
    require(
        change.path.join(SCRIPT).is_file(),
        "This checkout predates dispatchdev shot: rebase it onto origin/main.",
    )?;
    let running = api::Running::find(ws, name, runner)?;
    let (site, session) = api::sign_in(&running, who, open.dsp, runner)?;
    // A member's DSP is the address they signed in at; the owner's, the link's fragment names.
    let dsp = if who == OWNER && !open.platform {
        let dsps = session["dsps"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        Some(open.dsp_id(dsps)?)
    } else {
        None
    };
    let fragment = preview::fragment(open.page, dsp.as_deref(), open.tab);
    let url = format!("{}/{fragment}", site.origin);
    // The browser takes the session in a file of its own kind, as private as the jar.
    let storage = running.scratch.join(format!("shot-{who}.json"));
    let jar = std::fs::read_to_string(&site.jar)?;
    workspace::write_private(
        &storage,
        &json!({ "cookies": cookies(&jar), "origins": [] }).to_string(),
    )?;
    let png = running.scratch.join("shots").join(file_name(shot));
    let (storage, png) = (
        storage.to_str().ok_or("Non-UTF8 path")?,
        png.to_str().ok_or("Non-UTF8 path")?,
    );
    runner.command(
        &[
            "node",
            "node_modules/tsx/dist/cli.mjs",
            SCRIPT,
            "--url",
            &url,
            "--storage",
            storage,
            // A DSP's own address is a name under localhost, which the preview's host answers.
            "--host",
            &running.host,
            "--size",
            shot.size,
            "--theme",
            shot.theme,
            "--out",
            png,
        ],
        Some(&change.path),
        120,
    )?;
    println!("{png}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_browser_takes_the_session_curl_keeps() {
        let jar = "# Netscape HTTP Cookie File\n\n\
            #HttpOnly_preview.test\tFALSE\t/\tFALSE\t0\tdispatch_preview_4101\towner-session\n\
            nll.localhost\tFALSE\t/\tFALSE\t1798761600\tdispatch_preview_4101\tmember-session\n";
        assert_eq!(
            cookies(jar),
            [
                json!({"name":"dispatch_preview_4101","value":"owner-session","domain":"preview.test",
                    "path":"/","expires":-1,"httpOnly":true,"secure":false,"sameSite":"Strict"}),
                json!({"name":"dispatch_preview_4101","value":"member-session",
                    "domain":"nll.localhost","path":"/","expires":1798761600,"httpOnly":false,
                    "secure":false,"sameSite":"Strict"}),
            ]
        );
    }
}
