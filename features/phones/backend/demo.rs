//! What a development or preview DSP shows of Phones: a stand-in phone, which no app holds, with
//! a few texts it received this morning. A text Dispatch sends from it is delivered at once.
use super::storage::{PhonesStore, Reported};
use dispatch_core::{Result, db::{self, Store}};
use dispatch_phones_contract::Direction;

pub fn demo(db: &Store, dsp: &str, _timezone: &str) -> Result<()> {
    let now = db::now();
    let minutes = |m: i64| now - m * 60 * 1000;
    let received = [
        ("+12065550190", "Done with lunch, back on route", 62),
        ("+14255550199", "Hi is this dispatch? I start Monday", 41),
        ("+14255550131", "Gate code at Lakeview isn't working", 18),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (number, body, ago))| Reported {
        client_id: format!("demo-{index}"),
        direction: Direction::In,
        number: number.into(),
        body: body.into(),
        at: minutes(ago),
        files: Vec::new(),
    })
    .collect();
    db.phones_demo(dsp, "+12065550100", received, now)
}
