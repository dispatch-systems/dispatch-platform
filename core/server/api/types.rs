use crate::text_enum;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/server/api/generated/"))]
        pub enum Presence {
        Active => "active",
        Idle => "idle",
        Offline => "offline",
    }
}
