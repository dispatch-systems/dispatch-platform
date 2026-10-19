//! DVIC's settings, on its own page: when its reports are collected.
use dispatch_core::manifest::{Section, schedules, section};

/// The schedules of its `dvic` collection, which core's schedule routes keep.
pub const COLLECTION: Section = section(
    "dvic.collection",
    "Collection",
    "dvic.manage",
    &[schedules(
        "schedules",
        "Schedules",
        "/api/dsp/dvic/schedules",
        "dvic",
        ("Daily DVIC", "18:00"),
    )],
);
