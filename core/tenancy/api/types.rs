use crate::text_enum;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/tenancy/api/generated/"))]
        pub enum DspStatus {
        Provisioning => "provisioning",
        Active => "active",
        Suspended => "suspended",
        Failed => "failed",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/tenancy/api/generated/"))]
        pub enum OwnerStatus {
        Active => "active",
        Invited => "invited",
        Missing => "missing",
    }
}
