//! What every kind of conversation shares, as Communications keeps and reads them: what a
//! conversation is (`ConversationKind`), whether it waits on an answer (`ConversationState`), and
//! who wrote a message (`Direction`).
use dispatch_core::text_enum;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// What a conversation is: a Slack direct message, the texts of the work phones a driver
    /// held (or of one nobody held as they were written), or a number's texts.
    pub enum ConversationKind {
        Slack => "slack",
        Work => "work",
        Number => "number",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// Whether a conversation waits on an answer: the member's latest message came after
    /// anyone at the DSP answered, or marked it done; someone at the DSP answered last; or
    /// marked it done without answering.
    pub enum ConversationState {
        Waiting => "waiting",
        Responded => "responded",
        Done => "done",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// Who wrote a message: the member (`in`), or someone at the DSP (`out`).
    pub enum Direction {
        In => "in",
        Out => "out",
    }
}
