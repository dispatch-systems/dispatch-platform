//! Slack's text, as Communications keeps it and sends it. What a member writes comes in Slack's
//! markup: `&`, `<` and `>` escaped, and links, mentions and channels in angle brackets. It's
//! kept as it reads: a link by what it shows, with its address when that's another; a mention
//! by the name of whoever it names; a channel by its name. What someone at the DSP writes goes
//! out escaped, so Slack shows it as written.

/// `text` from Slack, as it reads, `named` giving the name of a member mentioned, by their ID.
pub fn plain(text: &str, named: impl Fn(&str) -> Option<String>) -> String {
    let mut read = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let Some(length) = rest[start..].find('>') else {
            break;
        };
        read.push_str(&unescaped(&rest[..start]));
        read.push_str(&unescaped(&marked(&rest[start + 1..start + length], &named)));
        rest = &rest[start + length + 1..];
    }
    read.push_str(&unescaped(rest));
    read
}

/// What one `<…>` of Slack's reads as: a mention, a channel, a word to all, or a link.
fn marked(inside: &str, named: &impl Fn(&str) -> Option<String>) -> String {
    let (target, label) = match inside.split_once('|') {
        Some((target, label)) => (target, Some(label).filter(|label| !label.is_empty())),
        None => (inside, None),
    };
    if let Some(user) = target.strip_prefix('@') {
        return format!("@{}", label.map(str::to_owned).or_else(|| named(user)).unwrap_or_else(|| user.to_owned()));
    }
    if let Some(channel) = target.strip_prefix('#') {
        return format!("#{}", label.unwrap_or(channel));
    }
    if let Some(special) = target.strip_prefix('!') {
        // A date or a group shows its own words; a word to all, as `@here`.
        return label
            .map(str::to_owned)
            .unwrap_or_else(|| format!("@{}", special.split('^').next().unwrap_or(special)));
    }
    let address = target.strip_prefix("mailto:").unwrap_or(target);
    match label {
        // A link shown by its address, as Slack writes one it found in what someone wrote.
        Some(label) if address.ends_with(label) || label == address => label.to_owned(),
        Some(label) => format!("{label} ({address})"),
        None => address.to_owned(),
    }
}

fn unescaped(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// `text` as Slack is to show it as written: `&`, `<` and `>` escaped.
pub fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
#[path = "../tests/backend/text.rs"]
mod tests;
