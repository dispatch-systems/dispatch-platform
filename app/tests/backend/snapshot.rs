//! What the app's tests hold the whole product to: lists that change with the features and
//! collectors it is made of, such as the catalog, the databases and their schemas. Each is a
//! file, so that such a change shows in review as a change to it. With
//! `DISPATCH_UPDATE_SNAPSHOTS` set, a test writes what it finds there instead of failing on a
//! difference: `npm run snapshots:update` sets it, and names each file that changed.
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The command that rewrites the snapshots, which a test names when one differs.
pub const UPDATE: &str = "npm run snapshots:update";

/// Whether the tests write what they find, rather than compare it with their snapshots.
pub fn updating() -> bool {
    std::env::var_os("DISPATCH_UPDATE_SNAPSHOTS").is_some()
}

/// The repository's root: it holds Cargo.lock, wherever this crate's manifest sits in it.
pub fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .expect("repository root")
}

/// Holds `found` to `name` in app/tests/backend/snapshots/, written as JSON.
#[track_caller]
pub fn check(name: &str, found: &Value) {
    let path: PathBuf = root().join("app/tests/backend/snapshots").join(name);
    compare(&path, &json(found));
}

/// Holds `found` to the file at `path`, or writes it there first while updating.
#[track_caller]
pub fn compare(path: &Path, found: &str) {
    if updating() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, found).unwrap();
    }
    let stored = std::fs::read_to_string(path).unwrap_or_default();
    let file = path.strip_prefix(root()).unwrap_or(path).display();
    assert!(
        stored == found,
        "{file} differs from what the test finds: `{UPDATE}` rewrites it, to review with the \
         change\n{}",
        diff(&stored, found)
    );
}

/// The lines that differ: `-` for what the file holds, `+` for what the test finds.
fn diff(stored: &str, found: &str) -> String {
    let (old, new): (Vec<&str>, Vec<&str>) = (stored.lines().collect(), found.lines().collect());
    // The longest run of lines both share, measured from each pair of places onwards.
    let mut shared = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            shared[i][j] = if old[i] == new[j] {
                shared[i + 1][j + 1] + 1
            } else {
                shared[i + 1][j].max(shared[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut lines) = (0, 0, vec![]);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            (i, j) = (i + 1, j + 1);
        } else if j < new.len() && (i == old.len() || shared[i][j + 1] >= shared[i + 1][j]) {
            lines.push(format!("+ {}", new[j]));
            j += 1;
        } else {
            lines.push(format!("- {}", old[i]));
            i += 1;
        }
    }
    lines.join("\n")
}

/// JSON as a reviewer reads it: each value on one line where that fits in 100 columns,
/// otherwise one item a line, as are the outermost and every list of lists or objects, so
/// that adding to a list adds a line.
pub fn json(value: &Value) -> String {
    let mut text = String::new();
    block(&mut text, value, 0, 0);
    text.push('\n');
    text
}
/// Writes `value` at `indent`, on one line if that takes at most `room` columns.
fn block(text: &mut String, value: &Value, indent: usize, room: usize) {
    let line = inline(value);
    let items: Vec<(Option<&String>, &Value)> = match value {
        Value::Array(items) => items.iter().map(|item| (None, item)).collect(),
        Value::Object(map) => map.iter().map(|(key, item)| (Some(key), item)).collect(),
        _ => vec![],
    };
    let records = matches!(value, Value::Array(items)
        if items.iter().all(|item| item.is_array() || item.is_object()));
    if items.is_empty() || (line.chars().count() <= room && !records) {
        text.push_str(&line);
        return;
    }
    let (open, close) = if value.is_array() {
        ('[', ']')
    } else {
        ('{', '}')
    };
    text.push(open);
    let inner = indent + 2;
    for (index, (key, item)) in items.iter().enumerate() {
        let last = index + 1 == items.len();
        let label = key.map_or_else(String::new, |key| {
            format!("{}: ", Value::from(key.as_str()))
        });
        text.push('\n');
        text.push_str(&" ".repeat(inner));
        text.push_str(&label);
        // The comma after each but the last takes a column too.
        let used = inner + label.chars().count() + usize::from(!last);
        block(text, item, inner, 100usize.saturating_sub(used));
        if !last {
            text.push(',');
        }
    }
    text.push('\n');
    text.push_str(&" ".repeat(indent));
    text.push(close);
}
/// `value` on one line, with a space after each comma and colon.
fn inline(value: &Value) -> String {
    match value {
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(inline).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(map) => {
            let fields: Vec<String> = map
                .iter()
                .map(|(key, item)| format!("{}: {}", Value::from(key.as_str()), inline(item)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
        scalar => scalar.to_string(),
    }
}
