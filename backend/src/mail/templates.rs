// Transactional email bodies. Mail clients ignore stylesheets and SVG, so the
// layout is nested tables with inline styles and the mark is a hosted PNG.
const FONT: &str =
    "Inter,-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif";
const INK: &str = "#171b25";
const MUTED: &str = "#626b7a";
const BORDER: &str = "#e3e7ee";
const PRIMARY: &str = "#2055ed";
const PAGE: &str = "#f4f6fa";
// Every email comes from a no-reply address; each one says so where a reader looks first
// when they want to answer.
const NO_REPLY: &str = "This is an automated email. Replies to it are not read.";
// Matches the dashboard DspAvatar tones as (ink, surface).
const TONES: [(&str, &str); 5] = [
    ("#51647d", "#eaf0f7"),
    ("#36765e", "#eaf4ee"),
    ("#6553aa", "#f0edf9"),
    ("#51647d", "#eaf0f7"),
    ("#427380", "#e9f3f5"),
];

pub struct Message {
    pub subject: String,
    pub text: String,
    pub html: String,
}

pub struct Invitation<'a> {
    pub origin: &'a str,
    pub dev: bool,
    pub to: &'a str,
    pub inviter: &'a str,
    pub dsp: &'a str,
    pub role: &'a str,
    pub url: &'a str,
    pub expires_at: i64,
    pub onboarding: bool,
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn strong(value: &str) -> String {
    format!(
        r#"<strong style="color:{INK};font-weight:600">{}</strong>"#,
        escape(value)
    )
}

// Same initials and tone selection as the dashboard DspAvatar.
fn avatar(name: &str) -> String {
    let upper = name.to_uppercase();
    let words: Vec<&str> = upper
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let initials: String = match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect(),
        [first, second, ..] => first
            .chars()
            .take(1)
            .chain(second.chars().take(1))
            .collect(),
    };
    let hash = words
        .join(" ")
        .chars()
        .fold(0u32, |hash, c| hash.wrapping_mul(31).wrapping_add(c as u32));
    let (ink, surface) = TONES[(hash % 5) as usize];
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin-bottom:22px"><tr><td width="48" height="48" align="center" style="width:48px;height:48px;border-radius:10px;background:{surface};color:{ink};font:600 15px {FONT};letter-spacing:-.2px">{}</td></tr></table>"#,
        escape(&initials)
    )
}

fn heading(value: &str) -> String {
    format!(
        r#"<h1 style="margin:0 0 10px;font:600 22px/1.3 {FONT};letter-spacing:-.4px;color:{INK}">{}</h1>"#,
        escape(value)
    )
}

fn action(label: &str, url: &str, note: &str) -> String {
    let url = escape(url);
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0"><tr><td style="border-radius:8px;background:{PRIMARY}"><a href="{url}" style="display:inline-block;padding:13px 24px;font:600 15px {FONT};color:#ffffff;text-decoration:none">{label}</a></td></tr></table><p style="margin:18px 0 0;font:400 13px/1.6 {FONT};color:{MUTED}">{note}</p><p style="margin:28px 0 0;padding-top:20px;border-top:1px solid {BORDER};font:400 12px/1.6 {FONT};color:{MUTED}">Button not working? Paste this link into your browser:<br><a href="{url}" style="color:{PRIMARY};text-decoration:none;word-break:break-all">{url}</a></p>"#
    )
}

fn shell(origin: &str, dev: bool, preheader: &str, body: &str, footer: &str) -> String {
    let pill = if dev {
        format!(
            r#" <span style="display:inline-block;vertical-align:middle;margin-left:8px;padding:2px 7px;border-radius:999px;background:#fdf0d5;color:#8a5a00;font:600 10px {FONT};letter-spacing:.6px">DEV</span>"#
        )
    } else {
        String::new()
    };
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><meta name="color-scheme" content="light dark"><meta name="supported-color-schemes" content="light dark"></head><body style="margin:0;padding:0;background:{PAGE}"><div style="display:none;max-height:0;overflow:hidden;opacity:0">{}</div><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:{PAGE}"><tr><td align="center" style="padding:40px 16px"><table role="presentation" width="520" cellpadding="0" cellspacing="0" style="width:100%;max-width:520px"><tr><td style="padding:0 4px 20px"><img src="{}/assets/email-mark.png" width="24" height="24" alt="" style="vertical-align:middle;border:0"><span style="vertical-align:middle;margin-left:9px;font:600 19px {FONT};letter-spacing:-.6px;color:{INK}">Dispatch</span>{pill}</td></tr><tr><td style="background:#ffffff;border:1px solid {BORDER};border-radius:12px;padding:36px 36px 32px">{body}</td></tr><tr><td style="padding:20px 4px 0;font:400 12px/1.6 {FONT};color:{MUTED}">{}</td></tr></table></td></tr></table></body></html>"#,
        escape(preheader),
        escape(origin),
        escape(footer)
    )
}

pub fn invitation(i: &Invitation) -> Message {
    // Roles are named by each DSP, so the article follows the name's first letter.
    let article = if i
        .role
        .starts_with(['A', 'E', 'I', 'O', 'U', 'a', 'e', 'i', 'o', 'u'])
    {
        "an"
    } else {
        "a"
    };
    let expires = chrono::DateTime::from_timestamp_millis(i.expires_at)
        .map(|at| at.format("%B %-d, %Y").to_string())
        .unwrap_or_default();
    // A DSP awaiting onboarding still has its placeholder name, so that copy never shows it.
    let (subject, label) = if i.onboarding {
        (
            "Set up your DSP on Dispatch".to_owned(),
            "Start DSP onboarding",
        )
    } else {
        (format!("Join {} on Dispatch", i.dsp), "Accept invitation")
    };
    let lead = |mark: &dyn Fn(&str) -> String| {
        let who = if i.inviter.is_empty() {
            "You've been invited".to_owned()
        } else {
            format!("{} invited you", mark(i.inviter))
        };
        if i.onboarding {
            format!(
                "{who} to set up a new DSP on Dispatch as its {}. Create your account, then finish configuring your DSP.",
                mark("owner")
            )
        } else {
            format!(
                "{who} to join {} as {article} {}.",
                mark(i.dsp),
                mark(i.role)
            )
        }
    };
    let (lead, lead_html) = (lead(&str::to_owned), lead(&strong));
    let note = format!("This invitation expires on {expires}.");
    let footer = format!(
        "This invitation was sent to {}. If you weren't expecting it, you can safely ignore this email. {NO_REPLY}",
        i.to
    );
    let body = format!(
        r#"{}{}<p style="margin:0 0 26px;font:400 15px/1.6 {FONT};color:{MUTED}">{lead_html}</p>{}"#,
        if i.onboarding {
            String::new()
        } else {
            avatar(i.dsp)
        },
        heading(&subject),
        action(label, i.url, &note)
    );
    Message {
        text: format!(
            "{subject}\n\n{lead}\n\n{label}: {}\n\n{note}\n\n{footer}",
            i.url
        ),
        html: shell(i.origin, i.dev, &lead, &body, &footer),
        subject,
    }
}

pub fn reset(origin: &str, dev: bool, to: &str, url: &str) -> Message {
    let note = "This link expires in 30 minutes and can only be used once.";
    let footer = format!(
        "If you didn't request a password reset, you can safely ignore this email. Your password won't change. {NO_REPLY}"
    );
    let body = format!(
        r#"{}<p style="margin:0 0 26px;font:400 15px/1.6 {FONT};color:{MUTED}">We received a request to reset the password for {}.</p>{}"#,
        heading("Reset your password"),
        strong(to),
        action("Reset password", url, note)
    );
    Message {
        subject: "Reset your Dispatch password".into(),
        text: format!(
            "Reset your password\n\nWe received a request to reset the password for {to}.\n\nReset password: {url}\n\n{note}\n\n{footer}"
        ),
        html: shell(
            origin,
            dev,
            "Reset your Dispatch password. This link expires in 30 minutes.",
            &body,
            &footer,
        ),
    }
}

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
    pub essential: bool,
    pub locations: bool,
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
        (
            "Tools",
            if app.essential { "Essential" } else { "Full" }.to_owned(),
        ),
        (
            "Delivery addresses and GPS",
            if app.locations {
                "Included"
            } else {
                "Not included"
            }
            .to_owned(),
        ),
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
    rows.push(("Disconnected", when(app.at)));
    app.message(
        subject,
        lead,
        &rows,
        "To keep using the app with Dispatch, connect it again from the app.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(onboarding: bool, inviter: &'static str) -> Message {
        invitation(&Invitation {
            origin: "https://dispatch.test",
            dev: false,
            to: "alex@dispatch.test",
            inviter,
            dsp: "North <Line> Logistics",
            role: "manager",
            url: "https://dispatch.test/#invite?token=abc",
            expires_at: 1_790_337_600_000,
            onboarding,
        })
    }
    #[test]
    fn avatar_matches_dashboard_initials_and_tone() {
        let html = avatar("Northline Logistics");
        assert!(html.contains(">NL<") && html.contains("#eaf0f7"));
        assert!(avatar("acme").contains(">AC<") && avatar("").contains(">?<"));
    }
    #[test]
    fn invitation_names_the_inviter_and_escapes_the_dsp() {
        let mail = sample(false, "Alex Morgan");
        assert_eq!(mail.subject, "Join North <Line> Logistics on Dispatch");
        assert!(
            mail.html.contains("North &lt;Line&gt; Logistics") && !mail.html.contains("<Line>")
        );
        assert!(mail.html.contains(">Accept invitation</a>"));
        assert!(mail.text.ends_with(NO_REPLY) && mail.html.contains(NO_REPLY));
        assert!(mail.text.contains("Alex Morgan invited you to join"));
        assert!(mail.text.contains("expires on September 25, 2026."));
        assert!(
            sample(false, "")
                .text
                .contains("You've been invited to join")
        );
    }
    #[test]
    fn onboarding_hides_the_placeholder_dsp_name() {
        let mail = sample(true, "Alex Morgan");
        assert_eq!(mail.subject, "Set up your DSP on Dispatch");
        assert!(
            mail.html.contains(">Start DSP onboarding</a>") && !mail.html.contains("Logistics")
        );
    }

    fn connected(known: bool, dsps: &[String]) -> ConnectedApp<'_> {
        ConnectedApp {
            origin: "https://dispatch.test",
            dev: false,
            to: "owner@dispatch.test",
            connection: "Laptop <Claude>",
            app: "Claude Code",
            known,
            destination: Some("this computer"),
            dsps,
            essential: true,
            locations: false,
            approved_by: "Platform Owner",
            at: 1_790_337_600_000,
        }
    }
    #[test]
    fn a_connected_app_is_described_plainly_and_escaped() {
        let mail = app_connected(&connected(true, &[]));
        assert_eq!(mail.subject, "Claude Code connected to Dispatch");
        for line in [
            "Connection: Laptop <Claude>",
            "App: Claude Code (known metadata)",
            "Sends access to: this computer",
            "DSPs: All DSPs",
            "Tools: Essential",
            "Delivery addresses and GPS: Not included",
            "Approved by: Platform Owner",
            "Connected: September 25, 2026 at 12:00 PM UTC",
            "Review connected apps: https://dispatch.test/#agents?tab=apps",
        ] {
            assert!(mail.text.contains(line), "{line}\n{}", mail.text);
        }
        assert!(mail.html.contains("Laptop &lt;Claude&gt;") && !mail.html.contains("<Claude>"));
        assert!(mail.text.ends_with(NO_REPLY) && mail.html.contains(NO_REPLY));
        let names: Vec<String> = (1..=12).map(|n| format!("DSP {n}")).collect();
        let mail = app_connected(&connected(false, &names));
        assert_eq!(
            mail.subject,
            "Claude Code (unrecognized) connected to Dispatch"
        );
        assert!(
            mail.text
                .contains("App: Unrecognized app, which says it is \u{201c}Claude Code\u{201d}")
        );
        assert!(mail.text.contains(
            "DSPs: DSP 1, DSP 2, DSP 3, DSP 4, DSP 5, DSP 6, DSP 7, DSP 8, DSP 9, DSP 10 and 2 more"
        ));
    }
    #[test]
    fn a_disconnected_app_says_why_in_plain_words() {
        let app = connected(true, &[]);
        let mail = app_disconnected(&app, "refresh_reused");
        assert_eq!(mail.subject, "Dispatch disconnected Claude Code");
        assert!(mail.text.contains("already used was presented again"));
        assert!(
            app_disconnected(&app, "code_reused")
                .text
                .contains("one-time code")
        );
        assert!(mail.text.ends_with(NO_REPLY));
    }

    #[test]
    fn every_email_says_replies_are_not_read() {
        let mail = reset(
            "https://dispatch.example",
            false,
            "owner@example.com",
            "https://dispatch.example/#reset?token=t",
        );
        assert!(mail.text.ends_with(NO_REPLY) && mail.html.contains(NO_REPLY));
    }
}
