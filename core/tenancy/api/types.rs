use crate::text_enum;

text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
        pub enum DspStatus {
        Provisioning => "provisioning",
        Active => "active",
        Suspended => "suspended",
        Failed => "failed",
    }
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
        pub enum OwnerStatus {
        Active => "active",
        Invited => "invited",
        Missing => "missing",
    }
}
