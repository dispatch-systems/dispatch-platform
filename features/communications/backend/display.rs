//! How Communications looks to each member, their own in Settings, on its Display page: the
//! fields Settings keeps for them (`MessageDisplay` reads them), which Communications' own
//! component draws with a preview of the page, and the page reads as it draws.
use crate::api::display::{
    BubbleShape, EmployeeColor, LineSpacing, ListPreview, ListRows, ListWidth, MessageLayout,
    MessageStyle, ProfileOpens, Spacing, TeamColor, TeamSide, Times,
};
use dispatch_core::manifest::{Section, choice, number, section, toggle};

pub const DISPLAY: Section = section(
    "communications.display",
    "Message display",
    "communications.use",
    &[
        number("text_size", "Text size", 12, 20, Some(14)).unit("px"),
        number("phone_text_size", "Text size on a phone", 12, 20, Some(14)).unit("px"),
        choice(
            "line_spacing",
            "Line spacing",
            &[
                (LineSpacing::Tight.as_str(), "Tight"),
                (LineSpacing::Normal.as_str(), "Normal"),
                (LineSpacing::Relaxed.as_str(), "Relaxed"),
            ],
            LineSpacing::Normal.as_str(),
        ),
        choice(
            "spacing",
            "Spacing",
            &[
                (Spacing::Compact.as_str(), "Compact"),
                (Spacing::Comfortable.as_str(), "Comfortable"),
                (Spacing::Spacious.as_str(), "Spacious"),
            ],
            Spacing::Comfortable.as_str(),
        ),
        choice(
            "layout",
            "Layout",
            &[
                (MessageLayout::Full.as_str(), "Full width"),
                (MessageLayout::Centred.as_str(), "Centred"),
            ],
            MessageLayout::Full.as_str(),
        ),
        number("column_width", "Column width", 600, 1200, Some(800)).unit("px"),
        choice(
            "style",
            "Style",
            &[
                (MessageStyle::Bubbles.as_str(), "Bubbles"),
                (MessageStyle::Plain.as_str(), "Plain"),
            ],
            MessageStyle::Bubbles.as_str(),
        ),
        choice(
            "team_side",
            "Your team",
            &[
                (TeamSide::Right.as_str(), "Right"),
                (TeamSide::Left.as_str(), "Left"),
            ],
            TeamSide::Right.as_str(),
        ),
        choice(
            "bubble_shape",
            "Bubble shape",
            &[
                (BubbleShape::Round.as_str(), "Round"),
                (BubbleShape::Soft.as_str(), "Soft"),
                (BubbleShape::Square.as_str(), "Square"),
            ],
            BubbleShape::Round.as_str(),
        ),
        number("bubble_width", "Bubble width", 50, 90, Some(70)).unit("%"),
        choice(
            "times",
            "Times",
            &[
                (Times::EachMessage.as_str(), "Each message"),
                (Times::EachGroup.as_str(), "Each group"),
                (Times::OnHover.as_str(), "On hover"),
            ],
            Times::EachGroup.as_str(),
        ),
        toggle("message_photos", "Photos", true),
        toggle("team_names", "Team names", true),
        choice(
            "team_color",
            "Your team's color",
            &[
                (TeamColor::Blue.as_str(), "Blue"),
                (TeamColor::Indigo.as_str(), "Indigo"),
                (TeamColor::Teal.as_str(), "Teal"),
                (TeamColor::Green.as_str(), "Green"),
                (TeamColor::Purple.as_str(), "Purple"),
                (TeamColor::Slate.as_str(), "Slate"),
            ],
            TeamColor::Blue.as_str(),
        ),
        choice(
            "employee_color",
            "Employees' color",
            &[
                (EmployeeColor::Gray.as_str(), "Gray"),
                (EmployeeColor::Blue.as_str(), "Blue"),
                (EmployeeColor::Warm.as_str(), "Warm"),
                (EmployeeColor::Green.as_str(), "Green"),
            ],
            EmployeeColor::Gray.as_str(),
        ),
        choice(
            "list_width",
            "List width",
            &[
                (ListWidth::Narrow.as_str(), "Narrow"),
                (ListWidth::Medium.as_str(), "Medium"),
                (ListWidth::Wide.as_str(), "Wide"),
            ],
            ListWidth::Medium.as_str(),
        ),
        choice(
            "list_rows",
            "Rows",
            &[
                (ListRows::Compact.as_str(), "Compact"),
                (ListRows::Comfortable.as_str(), "Comfortable"),
            ],
            ListRows::Comfortable.as_str(),
        ),
        choice(
            "list_preview",
            "Preview",
            &[
                (ListPreview::None.as_str(), "None"),
                (ListPreview::One.as_str(), "1 line"),
                (ListPreview::Two.as_str(), "2 lines"),
            ],
            ListPreview::One.as_str(),
        ),
        toggle("list_photos", "Photos in the list", true),
        toggle("waiting_time", "Waiting time", true),
        choice(
            "profile_opens",
            "Profile opens",
            &[
                (ProfileOpens::Beside.as_str(), "Beside"),
                (ProfileOpens::Over.as_str(), "Over"),
            ],
            ProfileOpens::Beside.as_str(),
        ),
        number("profile_width", "Profile width", 280, 420, Some(320)).unit("px"),
        toggle("profile_slide", "Slide in", true),
        toggle("keep_profile_open", "Keep profile open", false),
        toggle("enter_sends", "Enter sends", true),
    ],
)
.on("display")
.member()
.custom()
.keywords(&["text size", "font", "bubbles", "colors", "layout", "messages"]);

#[cfg(test)]
#[path = "../tests/backend/display.rs"]
mod tests;
