//! What Driver Match makes of the synthetic DSP, once every source holds its people: one
//! code each.
use crate::driver_match::DriverMatchStore;
use dispatch_core::{
    Result,
    db::Store,
    mcp::synthetic::{Made, Step, Synthetic, World},
};
use serde_json::json;

pub const SYNTHETIC: Synthetic = Synthetic {
    people: None,
    steps: &[Step {
        order: 60,
        run: matched,
    }],
};

fn matched(db: &Store, world: &mut World) -> Result<Made> {
    Ok(Some(("matched", json!(db.match_drivers(&world.dsp)?))))
}
