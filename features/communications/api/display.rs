//! How each member has Communications look, as Settings keeps it for them (`communications.display`):
//! its values by field, a desktop's and a phone's own, which the page and its preview in Settings
//! read. Each type is written to TypeScript in `generated/`.
use dispatch_core::text_enum;
use serde::{Deserialize, Serialize};

/// How the page draws for one member: each choice's value by its field's id, a desktop's, then a
/// phone's own of those that apply on one (`phone_<choice>`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
pub struct MessageDisplay {
    /// Its text size in pixels.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub text_size: i64,
    pub line_spacing: LineSpacing,
    pub spacing: Spacing,
    pub layout: MessageLayout,
    /// The messages' column, centred, in pixels.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub column_width: i64,
    pub style: MessageStyle,
    pub team_side: TeamSide,
    pub bubble_shape: BubbleShape,
    /// How wide a bubble grows, in percent of the conversation.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bubble_width: i64,
    pub times: Times,
    /// Photos beside messages, and the team's names over theirs.
    pub message_photos: bool,
    pub team_names: bool,
    pub team_color: TeamColor,
    pub employee_color: EmployeeColor,
    pub list_width: ListWidth,
    pub list_rows: ListRows,
    pub list_preview: ListPreview,
    /// Photos in the list, and how long each has waited.
    pub list_photos: bool,
    pub waiting_time: bool,
    pub profile_opens: ProfileOpens,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub profile_width: i64,
    pub profile_slide: bool,
    /// The profile stays open from one conversation to the next.
    pub keep_profile_open: bool,
    /// Enter sends a message; without, it starts a new line.
    pub enter_sends: bool,
    /// A new message from an employee plays a chime, and shows the browser's notification.
    pub sound: bool,
    pub notifications: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub phone_text_size: i64,
    pub phone_line_spacing: LineSpacing,
    pub phone_spacing: Spacing,
    pub phone_style: MessageStyle,
    pub phone_team_side: TeamSide,
    pub phone_bubble_shape: BubbleShape,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub phone_bubble_width: i64,
    /// Each message's or each run's: a phone has no hover.
    pub phone_times: Times,
    pub phone_message_photos: bool,
    pub phone_team_names: bool,
    pub phone_team_color: TeamColor,
    pub phone_employee_color: EmployeeColor,
    pub phone_list_rows: ListRows,
    pub phone_list_preview: ListPreview,
    pub phone_list_photos: bool,
    pub phone_waiting_time: bool,
    pub phone_profile_slide: bool,
    pub phone_enter_sends: bool,
}

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum LineSpacing { Tight => "tight", Normal => "normal", Relaxed => "relaxed", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum Spacing { Compact => "compact", Comfortable => "comfortable", Spacious => "spacious", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum MessageLayout { Full => "full", Centred => "centred", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum MessageStyle { Bubbles => "bubbles", Plain => "plain", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum TeamSide { Right => "right", Left => "left", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum BubbleShape { Round => "round", Soft => "soft", Square => "square", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum Times { EachMessage => "each_message", EachGroup => "each_group", OnHover => "on_hover", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum TeamColor {
        Blue => "blue",
        Indigo => "indigo",
        Teal => "teal",
        Green => "green",
        Purple => "purple",
        Slate => "slate",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum EmployeeColor { Gray => "gray", Blue => "blue", Warm => "warm", Green => "green", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum ListWidth { Narrow => "narrow", Medium => "medium", Wide => "wide", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum ListRows { Compact => "compact", Comfortable => "comfortable", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum ListPreview { None => "none", One => "one", Two => "two", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    pub enum ProfileOpens { Beside => "beside", Over => "over", }
}
