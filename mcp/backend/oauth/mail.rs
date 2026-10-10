//! The emails the platform owners get about connected apps, in core's layout: one when an
//! app connects, and one when Dispatch itself disconnects one.
use dispatch_core::server::mail::templates::{
    BORDER, FONT, INK, MUTED, Message, NO_REPLY, action, escape, heading, shell,
};

/// A connected app, as the notices to platform owners about it describe it.
pub struct ConnectedApp<'a> {
    pub origin: &'a str,
    pub dev: bool,
    pub to: &'a str,
    /// The connection's name, as the owner approved it.
    pub connection: &'a str,
    /// The app's name: Dispatch's for a known app, the app's own word for any other.
    pub app: &'a str,
    pub known: bool,
    /// Where it was sent its access: "this computer" or a website's host, when known.
    pub destination: Option<&'a str>,
    /// The DSPs it reaches by name; empty when it reaches all of them.
    pub dsps: &'a [String],
    /// The tools it may use, in words.
    pub tools: &'a str,
    pub approved_by: &'a str,
    /// When it connected or was disconnected, in milliseconds.
    pub at: i64,
}

/// Label and value rows, as a plain table in HTML and lines in text.
fn facts(rows: &[(&str, String)]) -> (String, String) {
    let text = rows
        .iter()
        .map(|(label, value)| format!("{label}: {value}"))
        .collect::<Vec<_>>()
        .join("\n");
    let html: String = rows
        .iter()
        .map(|(label, value)| {
            format!(
                "<tr><td style=\"padding:7px 16px 7px 0;border-bottom:1px solid {BORDER};\
                 font:400 13px/1.5 {FONT};color:{MUTED};white-space:nowrap;vertical-align:top\">\
                 {}</td><td style=\"padding:7px 0;border-bottom:1px solid {BORDER};\
                 font:500 14px/1.5 {FONT};color:{INK}\">{}</td></tr>",
                escape(label),
                escape(value)
            )
        })
        .collect();
    (
        text,
        format!(
            "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" \
             style=\"margin:0 0 26px;border-top:1px solid {BORDER}\">{html}</table>"
        ),
    )
}

fn when(at: i64) -> String {
    chrono::DateTime::from_timestamp_millis(at)
        .map(|at| at.format("%B %-d, %Y at %-I:%M %p UTC").to_string())
        .unwrap_or_default()
}

impl ConnectedApp<'_> {
    /// The app as the owner should read it: an unrecognized one only by its own word.
    fn app_line(&self) -> String {
        if self.known {
            format!("{} (known metadata)", self.app)
        } else {
            format!(
                "Unrecognized app, which says it is \u{201c}{}\u{201d}",
                self.app
            )
        }
    }
    fn reach(&self) -> String {
        const SHOWN: usize = 10;
        match self.dsps.len() {
            0 => "All DSPs".into(),
            count if count <= SHOWN => self.dsps.join(", "),
            count => format!(
                "{} and {} more",
                self.dsps[..SHOWN].join(", "),
                count - SHOWN
            ),
        }
    }
    fn link(&self) -> String {
        format!("{}/#agents?tab=apps", self.origin)
    }
    fn footer(&self) -> String {
        format!(
            "This notice was sent to {} because you are a platform owner on Dispatch. {NO_REPLY}",
            self.to
        )
    }
    fn message(
        &self,
        subject: String,
        lead: String,
        rows: &[(&str, String)],
        note: &str,
    ) -> Message {
        let label = "Review connected apps";
        let url = self.link();
        let (rows_text, rows_html) = facts(rows);
        let footer = self.footer();
        let body = format!(
            r#"{}<p style="margin:0 0 22px;font:400 15px/1.6 {FONT};color:{MUTED}">{}</p>{rows_html}{}"#,
            heading(&subject),
            escape(&lead),
            action(label, &url, &escape(note))
        );
        Message {
            text: format!(
                "{subject}\n\n{lead}\n\n{rows_text}\n\n{label}: {url}\n\n{note}\n\n{footer}"
            ),
            html: shell(self.origin, self.dev, &lead, &body, &footer),
            subject,
        }
    }
}

/// Tells a platform owner that an app connected to Dispatch, with what it reaches.
pub fn app_connected(app: &ConnectedApp) -> Message {
    let subject = if app.known {
        format!("{} connected to Dispatch", app.app)
    } else {
        format!("{} (unrecognized) connected to Dispatch", app.app)
    };
    let lead = format!(
        "{} connected to Dispatch with Sign in with Dispatch, as \u{201c}{}\u{201d}. It can use \
         Dispatch as described below until the connection is revoked.",
        app.app, app.connection
    );
    let mut rows = vec![
        ("Connection", app.connection.to_owned()),
        ("App", app.app_line()),
    ];
    rows.extend(app.destination.map(|to| ("Sends access to", to.to_owned())));
    rows.extend([
        ("DSPs", app.reach()),
        ("Tools", app.tools.to_owned()),
        ("Approved by", app.approved_by.to_owned()),
        ("Connected", when(app.at)),
    ]);
    app.message(
        subject,
        lead,
        &rows,
        "If you don't recognize this connection, revoke it on the Agents page. Its access stops at once.",
    )
}

/// Tells a platform owner that Dispatch ended a connected app itself, and why in plain words.
pub fn app_disconnected(app: &ConnectedApp, reason: &str) -> Message {
    let why = match reason {
        "code_reused" => {
            "the one-time code that connected it was used a second time, which can mean someone else had a copy of it"
        }
        "refresh_reused" => {
            "a sign-in renewal it had already used was presented again, which can mean someone else had a copy of it"
        }
        _ => "its sign-in could no longer be trusted",
    };
    let subject = format!("Dispatch disconnected {}", app.app);
    let lead = format!(
        "Dispatch ended the connection \u{201c}{}\u{201d} because {why}. Its access stopped at once.",
        app.connection
    );
    let mut rows = vec![
        ("Connection", app.connection.to_owned()),
        ("App", app.app_line()),
    ];
    rows.extend(app.destination.map(|to| ("Sent access to", to.to_owned())));
    rows.extend([
        ("Tools", app.tools.to_owned()),
        ("Disconnected", when(app.at)),
    ]);
    app.message(
        subject,
        lead,
        &rows,
        "To keep using the app with Dispatch, connect it again from the app.",
    )
}

#[cfg(test)]
#[path = "../../tests/backend/oauth/mail.rs"]
mod tests;
