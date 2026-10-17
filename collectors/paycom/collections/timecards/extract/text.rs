//! Text as the browser would show it: trimmed, cleaned, decoded, and read as a time or a number.
use super::*;

// JavaScript's `\s` and `trim`.
pub(super) fn space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
pub(super) fn trim(value: &str) -> &str {
    value.trim_matches(space)
}
/// `String(value).replace(/\s+/g, ' ').trim()`.
pub(super) fn clean(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut gap = false;
    for c in value.chars() {
        if space(c) {
            gap = true;
        } else {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        }
    }
    out
}
/// A JavaScript string's `length`.
pub(super) fn units(value: &str) -> usize {
    value.encode_utf16().count()
}
/// `value.slice(start)` in UTF-16 units.
pub(super) fn slice_from(value: &str, start: usize) -> &str {
    let mut count = 0;
    for (index, c) in value.char_indices() {
        if count >= start {
            return &value[index..];
        }
        count += c.len_utf16();
    }
    ""
}
pub(super) fn bounded(value: &str, max: usize) -> bool {
    units(value) <= max
        && !value.chars().any(
            |c| matches!(c, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'),
        )
}
/// `/^(0[1-9]|1[0-2]):[0-5][0-9] [AP]M$/`.
pub(super) fn time(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 8
        && matches!((b[0], b[1]), (b'0', b'1'..=b'9') | (b'1', b'0'..=b'2'))
        && b[2] == b':'
        && (b'0'..=b'5').contains(&b[3])
        && b[4].is_ascii_digit()
        && b[5] == b' '
        && matches!(b[6], b'A' | b'P')
        && b[7] == b'M'
}
/// `/^(0?[1-9]|1[0-2]):([0-5][0-9]) ([AP])M$/`, as the zero-padded time it shows.
pub(super) fn shown_time(value: &str) -> Option<String> {
    let (hour, rest) = value.split_once(':')?;
    let hour = match hour.as_bytes() {
        [h @ b'1'..=b'9'] => format!("0{}", *h as char),
        [b'0', b'1'..=b'9'] | [b'1', b'0'..=b'2'] => hour.to_owned(),
        _ => return None,
    };
    let r = rest.as_bytes();
    (r.len() == 5
        && (b'0'..=b'5').contains(&r[0])
        && r[1].is_ascii_digit()
        && r[2] == b' '
        && matches!(r[3], b'A' | b'P')
        && r[4] == b'M')
        .then(|| format!("{hour}:{rest}"))
}
/// `Number(text)` for the text of a cell.
pub(super) fn number(value: &str) -> Option<f64> {
    let text = trim(value);
    if text.is_empty() {
        return Some(0.);
    }
    match text {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    for (prefixes, radix) in [(["0x", "0X"], 16), (["0o", "0O"], 8), (["0b", "0B"], 2)] {
        if let Some(digits) = prefixes.iter().find_map(|p| text.strip_prefix(p)) {
            return (!digits.is_empty() && digits.len() <= 32)
                .then(|| u128::from_str_radix(digits, radix).ok())
                .flatten()
                .map(|n| n as f64);
        }
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let valid = digits(whole)
        && digits(fraction)
        && (!whole.is_empty() || !fraction.is_empty())
        && exponent.is_none_or(|e| {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            !e.is_empty() && digits(e)
        });
    valid.then(|| text.parse().ok()).flatten()
}
/// A cell's amount: `$` and `,` removed, finite and not negative.
pub(super) fn numeric(value: &str) -> Option<f64> {
    let text = clean(value);
    if text.is_empty() {
        return None;
    }
    number(&text.replace(['$', ','], "")).filter(|n| n.is_finite() && *n >= 0.)
}
/// `Number(value.toFixed(2))` for a finite, non-negative value.
pub(super) fn fixed(value: f64) -> f64 {
    if value >= 1e21 {
        return value;
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i64;
    let fraction = u128::from(bits & ((1 << 52) - 1));
    let (mantissa, exponent) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | 1 << 52, exponent - 1075)
    };
    let scaled = mantissa * 100;
    // The nearest hundredth; an exact tie goes to the larger one, as `toFixed` does.
    let hundredths = if exponent >= 0 {
        scaled << exponent as u32
    } else if -exponent > 100 {
        0
    } else {
        let shift = (-exponent) as u32;
        (scaled + (1u128 << (shift - 1))) >> shift
    };
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
        .parse()
        .unwrap_or(f64::NAN)
}
/// `/<br\s*\/?>/gi` replaced with a newline.
pub(super) fn line_breaks(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let matched = tail
            .get(..2)
            .filter(|name| name.eq_ignore_ascii_case("br"))
            .and_then(|_| {
                let after = tail[2..].trim_start_matches(space);
                let after = after.strip_prefix('/').unwrap_or(after);
                after.strip_prefix('>')
            });
        match matched {
            Some(after) => {
                out.push('\n');
                rest = after;
            }
            None => {
                out.push('<');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}
/// A `<div>`'s `textContent` after `innerHTML = value`.
pub(super) fn markup_text(value: &str) -> String {
    if !value.contains(['<', '&', '\r', '\0']) {
        return value.to_owned();
    }
    let options = ParseOpts {
        tree_builder: TreeBuilderOpts {
            scripting_enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let fragment = html5ever::parse_fragment(
        HtmlTreeSink::new(Html::new_fragment()),
        options,
        QualName::new(None, ns!(html), local_name!("div")),
        Vec::new(),
        false,
    )
    .one(value);
    text(fragment.tree.root(), true)
}
/// `timecard.js`'s `decode`: an attribute that holds markup, as the text it shows.
pub(super) fn decode(value: &str) -> String {
    let mut out = value.to_owned();
    for _ in 0..2 {
        out = markup_text(&line_breaks(&out));
    }
    let out = out.replace('\u{a0}', " ");
    let mut collapsed = String::with_capacity(out.len());
    let mut gap = false;
    for c in out.chars() {
        if c == ' ' || c == '\t' {
            gap = true;
        } else {
            if gap {
                collapsed.push(' ');
            }
            gap = false;
            collapsed.push(c);
        }
    }
    if gap {
        collapsed.push(' ');
    }
    trim(&collapsed).to_owned()
}
