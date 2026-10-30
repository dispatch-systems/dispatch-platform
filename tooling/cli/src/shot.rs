//! `dispatchdev shot`: a PNG of a page of the dev server `npm run dev` runs in the checkout,
//! kept in the checkout's scratch folder. The account is signed in as `dispatchdev api` signs it
//! in, once a run of the server, and the checkout's own `tooling/dev/shot.ts` opens the page with
//! that session in the browser tests' Chromium.
use crate::{
    Result, Runner,
    api::{self, OWNER, Server},
    require, workspace,
};
use serde_json::{Value, json};
use std::path::Path;

const SCRIPT: &str = "tooling/dev/shot.ts";

/// Where the page opens: a page, in a DSP or on the platform owner's own pages, on a tab of the
/// page.
pub struct Open<'a> {
    pub page: Option<&'a str>,
    /// The DSP, by its id or a unique part of its name: the fixture's own unless named.
    pub dsp: Option<&'a str>,
    /// The platform owner's own pages, outside any DSP, in place of a DSP's.
    pub platform: bool,
    pub tab: Option<&'a str>,
}
impl Open<'_> {
    /// Refuses what can't be opened together.
    pub fn check(&self) -> Result<()> {
        require(
            self.tab.is_none() || self.page.is_some(),
            "--tab is a tab of a page: name the page with --page too.",
        )?;
        require(
            self.dsp.is_none() || !self.platform,
            "Choose --dsp or --platform.",
        )
    }
    /// The id of the DSP it opens, of those a session lists: the one it names, else the first,
    /// which is the fixture's own, as the owner's list puts it first.
    pub fn dsp_id(&self, dsps: &[Value]) -> Result<String> {
        let dsp = match self.dsp {
            Some(wanted) => api::dsp(dsps, wanted)?,
            None => dsps.first().ok_or("The dev server has no DSP to open.")?,
        };
        Ok(dsp["id"].as_str().ok_or("A DSP without an id")?.to_owned())
    }
}
/// The page's fragment, as the app reads its address: `#<page>`, or a DSP's page
/// `#dsp/<id>/<page>`, with `?tab=<tab>` for a tab of it.
pub fn fragment(page: Option<&str>, dsp: Option<&str>, tab: Option<&str>) -> String {
    let page = page.map(|page| page.trim_start_matches('#'));
    let mut fragment = match (dsp, page) {
        (Some(id), page) => format!("#dsp/{}/{}", encoded(id), page.unwrap_or("")),
        (None, Some(page)) => format!("#{page}"),
        (None, None) => String::new(),
    };
    if let Some(tab) = tab {
        fragment.push_str(&format!("?tab={}", encoded(tab)));
    }
    fragment
}
/// A value as an address holds it, each byte but the unreserved ones escaped.
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

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

pub fn run(root: &Path, shot: &Shot<'_>, runner: &dyn Runner) -> Result<()> {
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
        root.join(SCRIPT).is_file(),
        "This checkout predates dispatchdev shot: rebase it onto origin/main.",
    )?;
    let server = Server::find(root)?;
    let (site, session) = api::sign_in(&server, who, open.dsp, runner)?;
    // A member's DSP is the address they signed in at; the owner's, the link's fragment names.
    let dsp = if who == OWNER && !open.platform {
        let dsps = session["dsps"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        Some(open.dsp_id(dsps)?)
    } else {
        None
    };
    let fragment = fragment(open.page, dsp.as_deref(), open.tab);
    let url = format!("{}/{fragment}", site.origin);
    // The browser takes the session in a file of its own kind, as private as the jar.
    let storage = server.scratch.join(format!("shot-{who}.json"));
    let jar = std::fs::read_to_string(&site.jar)?;
    workspace::write_private(
        &storage,
        &json!({ "cookies": cookies(&jar), "origins": [] }).to_string(),
    )?;
    let png = server.scratch.join("shots").join(file_name(shot));
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
            // A DSP's own address is a name under localhost, which the server's host answers.
            "--host",
            &server.host,
            "--size",
            shot.size,
            "--theme",
            shot.theme,
            "--out",
            png,
        ],
        Some(root),
        120,
    )?;
    println!("{png}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_page_opens_in_a_dsp_on_its_tab_as_the_app_reads_its_address() {
        assert_eq!(fragment(Some("dvic"), None, None), "#dvic");
        assert_eq!(
            fragment(Some("settings"), Some("dsp_2"), Some("uniform data")),
            "#dsp/dsp_2/settings?tab=uniform%20data"
        );
        assert_eq!(fragment(None, Some("dsp_2"), None), "#dsp/dsp_2/");
        assert_eq!(fragment(None, None, None), "");
    }
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
