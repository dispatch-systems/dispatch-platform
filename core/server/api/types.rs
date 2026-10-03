use crate::text_enum;

text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
        pub enum Presence {
        Active => "active",
        Idle => "idle",
        Offline => "offline",
    }
}
