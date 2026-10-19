//! The member's own sections of a DSP's Settings, each drawn by accounts' own panel: their
//! profile, their security and their theme.
use crate::{manifest::Section, manifest::section, tenancy::roles};

pub const PROFILE: Section = section("accounts.profile", "Profile", roles::ACCESS, &[])
    .on("general")
    .member()
    .custom()
    .keywords(&["name", "email", "photo"])
    .order(10);
pub const SECURITY: Section = section("accounts.security", "Security", roles::ACCESS, &[])
    .on("security")
    .member()
    .custom()
    .keywords(&[
        "password",
        "passkey",
        "authenticator",
        "sessions",
        "two-factor",
    ])
    .order(10);
pub const THEME: Section = section("accounts.theme", "Theme", roles::ACCESS, &[])
    .on("theme")
    .member()
    .custom()
    .keywords(&["appearance", "dark", "light"])
    .order(10);
