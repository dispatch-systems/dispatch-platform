//! A canvas as a page to keep as a document: Slack's HTML of it, made a whole page of plain HTML
//! that Google turns into a Doc. Its text, headings, lists, checklists, links, tables and
//! pictures keep their order; columns stack; mentions of the workspace's members read as their
//! names. Its pictures are Slack files, which `sync` downloads, and go in it as `data:`
//! addresses, never as links to Slack, which a document's readers can't open.
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use scraper::{ElementRef, Html, Node};
use std::collections::BTreeMap;

/// How wide a picture is shown at most, in pixels: a page's width less its margins.
const WIDEST: u32 = 624;
/// How Slack's HTML names the file a picture is, in its alternative text.
const PLACEHOLDER: &str = "_SLACK_FILE_ALT_PLACEHOLDER_";
/// The space Slack leaves in an otherwise empty line, which a document would show.
const ZERO_WIDTH: char = '\u{200B}';

/// A picture in a canvas, as it goes into the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub mime: String,
    pub bytes: Vec<u8>,
    /// Its size in pixels, when Slack said.
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// The Slack files the canvas `html` shows as pictures, in order, each once.
pub fn pictures(html: &str) -> Vec<String> {
    let page = Html::parse_fragment(html);
    let mut found = Vec::new();
    for element in page.root_element().descendent_elements() {
        if element.value().name() == "img"
            && let Some(file) = picture(element)
            && !found.contains(&file)
        {
            found.push(file);
        }
    }
    found
}

/// The workspace's members the canvas `html` mentions, by their Slack IDs, each once.
pub fn mentioned(html: &str) -> Vec<String> {
    let page = Html::parse_fragment(html);
    let mut found = Vec::new();
    for element in page.root_element().descendent_elements() {
        if element.value().name() == "a"
            && let Some(user) = mention(element)
            && !found.contains(&user)
        {
            found.push(user);
        }
    }
    found
}

/// The canvas `html` as a page: its pictures from `pictures`, by their Slack files, and each
/// member it mentions by their name in `names`. A picture missing from `pictures` is left out.
pub fn page(
    html: &str,
    pictures: &BTreeMap<String, Picture>,
    names: &BTreeMap<String, String>,
) -> String {
    let canvas = Html::parse_fragment(html);
    let mut page = Writing {
        out: String::from("<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>"),
        pictures,
        names,
    };
    page.within(canvas.root_element());
    page.out.push_str("</body></html>");
    page.out
}

/// The page as it's written.
struct Writing<'a> {
    out: String,
    pictures: &'a BTreeMap<String, Picture>,
    names: &'a BTreeMap<String, String>,
}
impl Writing<'_> {
    /// Writes what `element` holds.
    fn within(&mut self, element: ElementRef<'_>) {
        for child in element.children() {
            match child.value() {
                Node::Text(text) => self.text(text),
                Node::Element(_) => {
                    if let Some(child) = ElementRef::wrap(child) {
                        self.element(child);
                    }
                }
                _ => {}
            }
        }
    }
    /// Writes `element` as the page keeps it: what it is, if a document has it, else only what
    /// it holds.
    fn element(&mut self, element: ElementRef<'_>) {
        let name = element.value().name();
        match name {
            "h1" | "h2" | "h3" | "p" | "b" | "i" | "u" | "s" | "code" | "pre" | "blockquote"
            | "ul" | "ol" | "table" | "thead" | "tbody" | "tr" | "td" | "th" => {
                self.wrapped(name, element);
            }
            "h4" | "h5" | "h6" => self.wrapped("h3", element),
            "strong" => self.wrapped("b", element),
            "em" => self.wrapped("i", element),
            "del" | "strike" => self.wrapped("s", element),
            "li" => {
                self.out.push_str("<li>");
                if let Some(ticked) = checked(element) {
                    self.out.push_str(if ticked { "☑ " } else { "☐ " });
                }
                self.within(element);
                self.out.push_str("</li>");
            }
            "br" => self.out.push_str("<br>"),
            "hr" => self.out.push_str("<hr>"),
            "img" => self.picture(element),
            "a" => self.link(element),
            "input" => {}
            "script" | "style" | "template" | "iframe" | "object" | "embed" => {}
            _ => self.within(element),
        }
    }
    fn wrapped(&mut self, tag: &str, element: ElementRef<'_>) {
        self.out.push('<');
        self.out.push_str(tag);
        self.out.push('>');
        self.within(element);
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push('>');
    }
    fn text(&mut self, text: &str) {
        let shown: String = text.chars().filter(|c| *c != ZERO_WIDTH).collect();
        self.out.push_str(&escaped(&shown));
    }
    /// A link to the web or an email address, as itself; a mention, as the member's name; any
    /// other, as its words alone.
    fn link(&mut self, element: ElementRef<'_>) {
        if let Some(user) = mention(element) {
            let name = self.names.get(&user).map_or(user.as_str(), String::as_str);
            self.text(&format!("@{name}"));
            return;
        }
        let href = element.value().attr("href").filter(|href| {
            ["https://", "http://", "mailto:"]
                .iter()
                .any(|scheme| href.starts_with(scheme))
        });
        match href {
            Some(href) => {
                self.out.push_str("<a href=\"");
                self.out.push_str(&escaped(href));
                self.out.push_str("\">");
                self.within(element);
                self.out.push_str("</a>");
            }
            None => self.within(element),
        }
    }
    /// A picture, as its bytes, no wider than a page.
    fn picture(&mut self, element: ElementRef<'_>) {
        let Some(picture) = picture(element).and_then(|file| self.pictures.get(&file)) else {
            return;
        };
        self.out.push_str("<p><img src=\"data:");
        self.out.push_str(&picture.mime);
        self.out.push_str(";base64,");
        self.out.push_str(&B64.encode(&picture.bytes));
        self.out.push('"');
        if let (Some(width), Some(height)) = (picture.width, picture.height)
            && width > 0
        {
            let shown = width.min(WIDEST);
            let tall = u64::from(height) * u64::from(shown) / u64::from(width);
            self.out
                .push_str(&format!(" width=\"{shown}\" height=\"{tall}\""));
        }
        self.out.push_str("></p>");
    }
}

/// The Slack file a picture is: named in its alternative text, or else last in its address.
fn picture(element: ElementRef<'_>) -> Option<String> {
    let alt = element.value().attr("alt").unwrap_or("");
    let named = alt.strip_prefix(PLACEHOLDER).map(str::to_owned);
    let from_src = || {
        let src = element.value().attr("src")?;
        let last = src.split('?').next()?.rsplit('/').next()?;
        Some(last.to_owned())
    };
    named.or_else(from_src).filter(|file| file_id(file))
}
/// The member a link mentions, as Slack writes a mention: `@` and their ID, linking nowhere.
fn mention(element: ElementRef<'_>) -> Option<String> {
    if element.value().attr("href").is_some() {
        return None;
    }
    let text: String = element.text().collect();
    let user = text.trim().strip_prefix('@')?;
    member_id(user).then(|| user.to_owned())
}
/// Whether a list item is a checklist's, and ticked: as Slack marks it, with a box or a class.
fn checked(element: ElementRef<'_>) -> Option<bool> {
    let ticked = |name: &str| name == "checked" || name == "done";
    let item = element.value();
    if item.classes().any(ticked) || item.attr("data-checked") == Some("true") {
        return Some(true);
    }
    let boxed = element.descendent_elements().find(|inner| {
        inner.value().name() == "input" && inner.value().attr("type") == Some("checkbox")
    });
    if let Some(boxed) = boxed {
        return Some(boxed.value().attr("checked").is_some());
    }
    let listed = element
        .parent()
        .and_then(ElementRef::wrap)
        .is_some_and(|list| list.value().classes().any(|name| name.contains("check")));
    listed.then_some(false)
}
/// Whether `id` is a Slack file's ID, as Slack writes them.
fn file_id(id: &str) -> bool {
    id.len() >= 9
        && id.starts_with('F')
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
/// Whether `id` is a workspace member's ID, as Slack writes them.
fn member_id(id: &str) -> bool {
    id.len() >= 9
        && (id.starts_with('U') || id.starts_with('W'))
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
#[path = "tests/backend/canvas_page.rs"]
mod tests;
