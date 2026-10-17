//! The document's elements, read as scripting off would read them: their text, attributes and
//! form values, and what a row's cells say.
use super::*;

/// Nodes below `root` in tree order, as a selector sees them: never inside a
/// template's contents.
pub(super) fn descendants<'a>(root: NodeRef<'a, Node>) -> impl Iterator<Item = NodeRef<'a, Node>> {
    let mut stack: Vec<_> = root.children().rev().collect();
    std::iter::from_fn(move || {
        let node = stack.pop()?;
        if !matches!(node.value(), Node::Fragment) {
            stack.extend(node.children().rev());
        }
        Some(node)
    })
}
pub(super) fn elements<'a>(root: NodeRef<'a, Node>) -> impl Iterator<Item = ElementRef<'a>> {
    descendants(root).filter_map(ElementRef::wrap)
}
/// `textContent`, or with `code` false the text `visible` keeps: no script or style.
pub(super) fn text(root: NodeRef<'_, Node>, code: bool) -> String {
    let mut out = String::new();
    let mut stack: Vec<_> = root.children().rev().collect();
    while let Some(node) = stack.pop() {
        match node.value() {
            Node::Text(value) => out.push_str(value),
            Node::Element(element) if !code && matches!(element.name(), "script" | "style") => {}
            Node::Element(_) => stack.extend(node.children().rev()),
            _ => {}
        }
    }
    out
}
pub(super) fn visible(element: Option<ElementRef<'_>>) -> String {
    element.map(|e| clean(&text(*e, false))).unwrap_or_default()
}
pub(super) fn is(element: ElementRef<'_>, name: &str) -> bool {
    element.value().name() == name
}
pub(super) fn html(element: ElementRef<'_>, name: &str) -> bool {
    is(element, name) && element.value().name.ns == ns!(html)
}
pub(super) fn attr<'a>(element: ElementRef<'a>, name: &str) -> Option<&'a str> {
    element.value().attr(name)
}
/// An option's `text`: whitespace stripped and collapsed, scripts left out.
pub(super) fn option_text(option: ElementRef<'_>) -> String {
    let mut out = String::new();
    let mut stack: Vec<_> = option.children().rev().collect();
    while let Some(node) = stack.pop() {
        match node.value() {
            Node::Text(value) => out.push_str(value),
            Node::Element(element) if element.name() == "script" => {}
            Node::Element(_) => stack.extend(node.children().rev()),
            _ => {}
        }
    }
    out.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}
pub(super) fn option_value(option: ElementRef<'_>) -> String {
    attr(option, "value").map_or_else(|| option_text(option), str::to_owned)
}
/// The option a parsed `<select>` has selected, and its `value`.
pub(super) fn selected(select: ElementRef<'_>) -> Option<ElementRef<'_>> {
    let mut options = Vec::new();
    for child in select.child_elements() {
        if html(child, "option") {
            options.push((child, attr(child, "disabled").is_some()));
        } else if html(child, "optgroup") {
            let disabled = attr(child, "disabled").is_some();
            for option in child.child_elements().filter(|e| html(*e, "option")) {
                options.push((option, disabled || attr(option, "disabled").is_some()));
            }
        }
    }
    let marked = || {
        options
            .iter()
            .filter(|(o, _)| attr(*o, "selected").is_some())
    };
    if attr(select, "multiple").is_some() {
        return marked().next().map(|(o, _)| *o);
    }
    // The rules for parsing non-negative integers: leading digits, the rest ignored.
    let size = attr(select, "size")
        .map(|v| v.trim_start_matches(|c: char| c.is_ascii_whitespace()))
        .map(|v| v.strip_prefix('+').unwrap_or(v))
        .map(|v| v.bytes().take_while(u8::is_ascii_digit).collect::<Vec<_>>())
        .filter(|digits| !digits.is_empty())
        .and_then(|digits| String::from_utf8(digits).ok()?.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(1);
    marked().next_back().map(|(o, _)| *o).or_else(|| {
        (size == 1)
            .then(|| {
                options
                    .iter()
                    .find(|(_, disabled)| !disabled)
                    .map(|(o, _)| *o)
            })
            .flatten()
    })
}
pub(super) fn select_value(select: ElementRef<'_>) -> String {
    selected(select).map(option_value).unwrap_or_default()
}
/// An `<input>`'s `value` before anyone edits it.
pub(super) fn input_value(input: ElementRef<'_>) -> Read<String> {
    let value = attr(input, "value");
    let kind = attr(input, "type").unwrap_or("").to_ascii_lowercase();
    let lines = |v: Option<&str>| v.unwrap_or("").replace(['\r', '\n'], "");
    Ok(match kind.as_str() {
        "checkbox" | "radio" => value.unwrap_or("on").to_owned(),
        "hidden" | "submit" | "image" | "reset" | "button" => value.unwrap_or("").to_owned(),
        "file" => String::new(),
        "url" => lines(value).trim_ascii().to_owned(),
        "email" if attr(input, "multiple").is_none() => lines(value).trim_ascii().to_owned(),
        // Each sanitizes its value by rules this reader does not repeat.
        "email" | "number" | "range" | "color" | "date" | "month" | "week" | "time"
        | "datetime-local" => return invalid("timecard_control_unsupported"),
        _ => lines(value),
    })
}
pub(super) fn textarea_value(textarea: ElementRef<'_>) -> String {
    textarea
        .children()
        .filter_map(|node| node.value().as_text().map(|t| t.to_string()))
        .collect()
}
/// `element.value`, where an element has one.
pub(super) fn value_property(element: ElementRef<'_>) -> Read<Option<String>> {
    if element.value().name.ns != ns!(html) {
        return Ok(None);
    }
    Ok(Some(match element.value().name() {
        "input" => input_value(element)?,
        "textarea" => textarea_value(element),
        "select" => select_value(element),
        "option" => option_value(element),
        "button" | "data" | "param" => attr(element, "value").unwrap_or("").to_owned(),
        "output" => text(*element, true),
        "li" | "meter" | "progress" => return invalid("timecard_control_unsupported"),
        _ => return Ok(None),
    }))
}
/// `/^Comment:\s*/i` removed.
pub(super) fn comment(value: &str) -> &str {
    match value.get(..8) {
        Some(prefix) if prefix.eq_ignore_ascii_case("comment:") => {
            value[8..].trim_start_matches(space)
        }
        _ => value,
    }
}
/// A day's heading, such as `SUN (09/13)`: its label and date.
pub(super) fn day_header(value: &str) -> Option<(String, String)> {
    let text = clean(value);
    let b = text.as_bytes();
    if b.len() < 3 || !b[..3].iter().all(u8::is_ascii_uppercase) {
        return None;
    }
    let rest = &text[3..];
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    let r = rest.as_bytes();
    (r.len() == 7
        && r[0] == b'('
        && r[1..3].iter().all(u8::is_ascii_digit)
        && r[3] == b'/'
        && r[4..6].iter().all(u8::is_ascii_digit)
        && r[6] == b')')
        .then(|| (text[..3].to_owned(), rest[1..6].to_owned()))
}
pub(super) fn weekly(row: ElementRef<'_>) -> bool {
    row.child_elements()
        .next()
        .is_some_and(|cell| clean(&text(*cell, true)).eq_ignore_ascii_case("Weekly Totals"))
}
/// `/^(IN DAY|OUT LUNCH|IN LUNCH|OUT DAY|OUT BREAK|IN BREAK)\b/i`, in its canonical kind.
pub(super) fn punch_kind(line: &str) -> String {
    for name in [
        "IN DAY",
        "OUT LUNCH",
        "IN LUNCH",
        "OUT DAY",
        "OUT BREAK",
        "IN BREAK",
    ] {
        let bytes = line.as_bytes();
        if bytes.len() >= name.len()
            && bytes[..name.len()].eq_ignore_ascii_case(name.as_bytes())
            && bytes
                .get(name.len())
                .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
        {
            return match name {
                "OUT BREAK" => "OUT LUNCH",
                "IN BREAK" => "IN LUNCH",
                other => other,
            }
            .to_owned();
        }
    }
    String::new()
}
/// A clock such as `Front Door (12)`: its name and code.
pub(super) fn clock(value: &str) -> (String, String) {
    let whole = value.trim_end_matches(space);
    if let Some(inner) = whole.strip_suffix(')')
        && let Some(open) = inner.rfind('(')
        && !inner[open + 1..].contains(')')
    {
        return (clean(&inner[..open]), clean(&inner[open + 1..]));
    }
    (value.to_owned(), String::new())
}
