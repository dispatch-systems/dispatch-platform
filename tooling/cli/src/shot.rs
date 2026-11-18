//! `dispatchdev shot`: a PNG of a page of the dev server `npm run dev` runs in the checkout,
//! kept in the checkout's scratch folder. The account is signed in as `dispatchdev api` signs it
//! in, once a run of the server, and the checkout's own `tooling/dev/shot.ts` opens the page with
//! that session in the browser tests' Chromium, on the tab it names, and clicks what it names,
//! such as what opens a dialog, before the shot: a pixel of the PNG to a pixel of the window.
use crate::{
    Result, Runner,
    api::{self, OWNER, Server},
    require, workspace,
};
use serde_json::{Value, json};
use std::path::Path;

const SCRIPT: &str = "tooling/dev/shot.ts";
/// The window a desktop screenshot is taken in: the owner's own screen, as they see a page.
pub const DESKTOP: &str = "1920x1080";
/// The window a phone screenshot is taken in.
pub const PHONE: &str = "390x844";

/// Where the page opens: a page, in a DSP or on the platform owner's own pages, on a tab of the
/// page.
pub struct Open<'a> {
    /// The page's address, with what it is asked after `?`: `conduct?driver=W5DHNV`.
    pub page: Option<&'a str>,
    /// The DSP, by its id or a unique part of its name: the fixture's own unless named.
    pub dsp: Option<&'a str>,
    /// The platform owner's own pages, outside any DSP, in place of a DSP's.
    pub platform: bool,
    pub tab: Option<&'a str>,
    /// A widget by its id, at every size it declares, as Home shows it off Production: each grid of
    /// them a screenshot.
    pub widget: Option<&'a str>,
}
/// Home's page, which shows a widget at every size with `?sizes=<id>`.
const HOME: &str = "overview";
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
        )?;
        require(
            self.widget.is_none() || (self.page.is_none() && self.tab.is_none() && !self.platform),
            "--widget opens Home on the widget's sizes: leave out --page, --tab and --platform.",
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
/// `#dsp/<id>/<page>`, with `tab=<tab>` asked of it too for a tab of it.
pub fn fragment(page: Option<&str>, dsp: Option<&str>, tab: Option<&str>) -> String {
    let page = page.map(|page| page.trim_start_matches('#'));
    let mut fragment = match (dsp, page) {
        (Some(id), page) => format!("#dsp/{}/{}", encoded(id), page.unwrap_or("")),
        (None, Some(page)) => format!("#{page}"),
        (None, None) => String::new(),
    };
    if let Some(tab) = tab {
        let joined = if fragment.contains('?') { '&' } else { '?' };
        fragment.push_str(&format!("{joined}tab={}", encoded(tab)));
    }
    fragment
}
/// Home's fragment showing `widget` at every size, in the DSP `dsp` or the one signed in at.
pub fn sizes(dsp: Option<&str>, widget: &str) -> String {
    format!(
        "{}?sizes={}",
        fragment(Some(HOME), dsp, None),
        encoded(widget)
    )
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
    /// What to click once the page is ready, each by its accessible name, in turn.
    pub clicks: &'a [&'a str],
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
        open.widget,
        open.dsp,
        open.platform.then_some("platform"),
    ]
    .into_iter()
    .chain(shot.clicks.iter().map(|click| Some(*click)))
    .chain([
        shot.who.and_then(|email| email.split('@').next()),
        Some(shot.size),
        Some(shot.theme),
    ]);
    let name = words
        .flatten()
        .flat_map(|word| word.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("{}.png", name.to_lowercase())
}

/// The width and height of the PNG `png` holds, from its header.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    let chunks = png.strip_prefix(b"\x89PNG\r\n\x1a\n")?;
    // The first chunk is its header: its length, `IHDR`, then the width and the height.
    let header = chunks
        .get(4..16)
        .filter(|header| header.starts_with(b"IHDR"))?;
    let side = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().expect("4 bytes"));
    Some((side(4), side(8)))
}
/// Refuses a PNG that isn't `size`, `<width>x<height>`, a pixel of it to a pixel of the window.
fn matches_window(png: &[u8], size: &str) -> Result<()> {
    let (width, height) = png_size(png).ok_or("The screenshot isn't a PNG.")?;
    require(
        format!("{width}x{height}") == size,
        &format!("The screenshot is {width}x{height}, not the window's {size}."),
    )
}

/// Saves the screenshot and prints its path. Answers with the fragment the page opened at.
pub fn run(root: &Path, shot: &Shot<'_>, runner: &dyn Runner) -> Result<String> {
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
    let fragment = match open.widget {
        Some(widget) => sizes(dsp.as_deref(), widget),
        None => fragment(open.page, dsp.as_deref(), open.tab),
    };
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
    let mut args = vec![
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
    ];
    // The address opens Settings' pages and the tabs other owners add; the script clicks the rest.
    if let Some(tab) = open.tab {
        args.extend(["--tab", tab]);
    }
    for click in shot.clicks {
        args.extend(["--click", click]);
    }
    // Each grid of a widget's sizes, the script turning to the next and saving it beside the first.
    if open.widget.is_some() {
        args.push("--pages");
    }
    // What the script says of the page, as a tab it lacks.
    let said = runner.command(&args, Some(root), 120)?;
    let said = String::from_utf8_lossy(&said);
    if !said.trim().is_empty() {
        println!("{}", said.trim());
    }
    matches_window(&std::fs::read(png)?, shot.size)?;
    // The script names each grid's screenshot it saved.
    if open.widget.is_none() {
        println!("{png}");
    }
    Ok(fragment)
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
        // A page asked for a record opens on it, on its tab too.
        assert_eq!(
            fragment(Some("conduct?driver=W5DHNV"), Some("dsp_2"), Some("notes")),
            "#dsp/dsp_2/conduct?driver=W5DHNV&tab=notes"
        );
    }
    #[test]
    fn a_screenshot_is_the_windows_size_a_pixel_to_a_pixel() {
        let png = |width: u32, height: u32| {
            let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
            png.extend(width.to_be_bytes());
            png.extend(height.to_be_bytes());
            png
        };
        assert!(matches_window(&png(1920, 1080), DESKTOP).is_ok());
        assert!(matches_window(&png(390, 844), PHONE).is_ok());
        // Taken at twice the pixels, as a screen of a higher density shows it.
        let doubled = matches_window(&png(3840, 2160), DESKTOP)
            .unwrap_err()
            .to_string();
        assert!(
            doubled.contains("3840x2160, not the window's 1920x1080"),
            "{doubled}"
        );
        assert!(matches_window(b"GIF89a", DESKTOP).is_err());
    }
    #[test]
    fn a_widget_opens_on_home_at_every_size() {
        assert_eq!(
            sizes(Some("dsp_2"), "dvic.latest"),
            "#dsp/dsp_2/overview?sizes=dvic.latest"
        );
        assert_eq!(sizes(None, "dvic.latest"), "#overview?sizes=dvic.latest");
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
