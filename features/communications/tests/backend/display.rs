//! A member's own Message display: what Settings keeps for them reads as the page reads it, each
//! field by its id, a desktop's and a phone's, the defaults until they save, and theirs alone.
use super::DISPLAY;
use crate::api::display::{
    BubbleShape, EmployeeColor, LineSpacing, ListPreview, ListRows, ListWidth, MessageDisplay,
    MessageLayout, MessageStyle, ProfileOpens, Spacing, TeamColor, TeamSide, Times,
};
use dispatch_core::{settings, testing};

#[test]
fn each_members_display_reads_as_they_saved_it_each_field_by_its_id() {
    crate::backend::support::install();
    let (_root, db, dsp) = testing::bootstrapped();
    // The platform's one user, and another member's id: each member's own are kept by their id.
    let (member, other) = (
        testing::platform_owner(&db),
        "usr_another_member".to_owned(),
    );
    let read = |member: &str| {
        settings::read_member::<MessageDisplay>(&db, &dsp, member, &DISPLAY).unwrap()
    };
    let defaults = MessageDisplay {
        text_size: 14,
        line_spacing: LineSpacing::Normal,
        spacing: Spacing::Comfortable,
        layout: MessageLayout::Full,
        column_width: 800,
        style: MessageStyle::Bubbles,
        team_side: TeamSide::Right,
        bubble_shape: BubbleShape::Round,
        bubble_width: 70,
        times: Times::EachGroup,
        message_photos: true,
        team_names: true,
        team_color: TeamColor::Blue,
        employee_color: EmployeeColor::Gray,
        list_width: ListWidth::Medium,
        list_rows: ListRows::Comfortable,
        list_preview: ListPreview::One,
        list_photos: true,
        waiting_time: true,
        profile_opens: ProfileOpens::Beside,
        profile_width: 320,
        profile_slide: true,
        keep_profile_open: false,
        enter_sends: true,
        phone_text_size: 14,
        phone_line_spacing: LineSpacing::Normal,
        phone_spacing: Spacing::Comfortable,
        phone_style: MessageStyle::Bubbles,
        phone_team_side: TeamSide::Right,
        phone_bubble_shape: BubbleShape::Round,
        phone_bubble_width: 80,
        phone_times: Times::EachGroup,
        phone_message_photos: true,
        phone_team_names: true,
        phone_team_color: TeamColor::Blue,
        phone_employee_color: EmployeeColor::Gray,
        phone_list_rows: ListRows::Comfortable,
        phone_list_preview: ListPreview::One,
        phone_list_photos: true,
        phone_waiting_time: true,
        phone_profile_slide: true,
        phone_enter_sends: false,
    };
    assert_eq!(read(&member), defaults);

    // Every field changed at once reads back as saved.
    let chosen = MessageDisplay {
        text_size: 16,
        line_spacing: LineSpacing::Relaxed,
        spacing: Spacing::Spacious,
        layout: MessageLayout::Centred,
        column_width: 900,
        style: MessageStyle::Plain,
        team_side: TeamSide::Left,
        bubble_shape: BubbleShape::Square,
        bubble_width: 60,
        times: Times::OnHover,
        message_photos: false,
        team_names: false,
        team_color: TeamColor::Green,
        employee_color: EmployeeColor::Warm,
        list_width: ListWidth::Wide,
        list_rows: ListRows::Compact,
        list_preview: ListPreview::Two,
        list_photos: false,
        waiting_time: false,
        profile_opens: ProfileOpens::Over,
        profile_width: 380,
        profile_slide: false,
        keep_profile_open: true,
        enter_sends: false,
        phone_text_size: 15,
        phone_line_spacing: LineSpacing::Tight,
        phone_spacing: Spacing::Compact,
        phone_style: MessageStyle::Plain,
        phone_team_side: TeamSide::Left,
        phone_bubble_shape: BubbleShape::Soft,
        phone_bubble_width: 95,
        phone_times: Times::EachMessage,
        phone_message_photos: false,
        phone_team_names: false,
        phone_team_color: TeamColor::Purple,
        phone_employee_color: EmployeeColor::Blue,
        phone_list_rows: ListRows::Compact,
        phone_list_preview: ListPreview::None,
        phone_list_photos: false,
        phone_waiting_time: false,
        phone_profile_slide: false,
        phone_enter_sends: true,
    };
    let values = serde_json::to_value(&chosen).unwrap();
    settings::save(&db, &dsp, &member, &DISPLAY, &values, 0, false).unwrap();
    assert_eq!(read(&member), chosen);
    // Another member still has the defaults.
    assert_eq!(read(&other), defaults);
}
