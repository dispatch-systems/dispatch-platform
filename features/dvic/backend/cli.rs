//! DVIC's operator commands.
use super::hidden;
use dispatch_core::{
    Result,
    collection::registry::added_identity,
    db::{self, iso},
    ensure,
    foundation::config::Config,
    tenancy::dsps,
};
use dispatch_cortex as cortex;
use serde_json::Value;

/// The operator's hidden DVIC drivers: `dvic-hidden <dsp>` lists them, `dvic-hide <dsp>
/// <driver> <note>` hides one and removes what was stored of them, `dvic-unhide <dsp>
/// <driver>` lets their later reports in. Each is one transaction on that DSP's DVIC
/// database, which SQLite serializes with the running server's writes, so it needs no
/// stopped service. It reads the platform's DSP list read-only and migrates nothing.
pub fn run(config: &Config, args: &[String]) -> Result<Value> {
    let usage = match args[0].as_str() {
        "dvic-hidden" => args.len() == 2,
        "dvic-hide" => args.len() == 4,
        "dvic-unhide" => args.len() == 3,
        _ => false,
    };
    ensure(usage, "usage_dvic_hidden_hide_unhide", 400)?;
    let dsp = args[1].as_str();
    ensure(db::identifier(dsp, "dsp_"), "invalid_dsp_id", 400)?;
    dsps::ensure_listed(config, dsp)?;
    let path = config
        .root
        .join("dsps")
        .join(dsp)
        .join("data/dvic/dvic.sqlite");
    ensure(path.is_file(), "dvic_storage_missing", 404)?;
    db::private_file(&path, false)?;
    let dvic = db::Db::open(&path, super::DATABASE)?;
    added_identity(&dvic, dsp, cortex::PROVIDER, &super::STORAGE)?;
    // The server adds the table when it starts the release that has it.
    ensure(
        dvic.one(
            "SELECT name FROM sqlite_master WHERE name='dvic_hidden_drivers'",
            [],
        )?
        .is_some(),
        "dvic_not_migrated",
        409,
    )?;
    match args[0].as_str() {
        "dvic-hide" => hidden::hide(&dvic, &args[2], &args[3], &iso()),
        "dvic-unhide" => hidden::unhide(&dvic, &args[2]),
        _ => hidden::list(&dvic),
    }
}
