//! How Communications looks to each member, their own in Settings, on its Display page: the
//! fields Settings keeps for them (`MessageDisplay` reads them), a desktop's and then a phone's
//! own of the choices that apply on one, which Communications' own component draws with a preview
//! of the page, and the page reads as it draws.
use crate::api::display::{
    BubbleShape, EmployeeColor, LineSpacing, ListPreview, ListRows, ListWidth, MessageLayout,
    MessageStyle, ProfileOpens, Spacing, TeamColor, TeamSide, Times,
};
use dispatch_core::manifest::{Section, choice, number, section, toggle};

// The choices of each, a desktop's and a phone's alike.
type Options = &'static [(&'static str, &'static str)];
const LINE_SPACING: Options = &[
    (LineSpacing::Tight.as_str(), "Tight"),
    (LineSpacing::Normal.as_str(), "Normal"),
    (LineSpacing::Relaxed.as_str(), "Relaxed"),
];
const SPACING: Options = &[
    (Spacing::Compact.as_str(), "Compact"),
    (Spacing::Comfortable.as_str(), "Comfortable"),
    (Spacing::Spacious.as_str(), "Spacious"),
];
const STYLE: Options = &[
    (MessageStyle::Bubbles.as_str(), "Bubbles"),
    (MessageStyle::Plain.as_str(), "Plain"),
];
const TEAM_SIDE: Options = &[
    (TeamSide::Right.as_str(), "Right"),
    (TeamSide::Left.as_str(), "Left"),
];
const BUBBLE_SHAPE: Options = &[
    (BubbleShape::Round.as_str(), "Round"),
    (BubbleShape::Soft.as_str(), "Soft"),
    (BubbleShape::Square.as_str(), "Square"),
];
/// A phone's times, which have no hover to show them on.
const PHONE_TIMES: Options = &[
    (Times::EachMessage.as_str(), "Each message"),
    (Times::EachGroup.as_str(), "Each group"),
];
const TEAM_COLOR: Options = &[
    (TeamColor::Blue.as_str(), "Blue"),
    (TeamColor::Indigo.as_str(), "Indigo"),
    (TeamColor::Teal.as_str(), "Teal"),
    (TeamColor::Green.as_str(), "Green"),
    (TeamColor::Purple.as_str(), "Purple"),
    (TeamColor::Slate.as_str(), "Slate"),
];
const EMPLOYEE_COLOR: Options = &[
    (EmployeeColor::Gray.as_str(), "Gray"),
    (EmployeeColor::Blue.as_str(), "Blue"),
    (EmployeeColor::Warm.as_str(), "Warm"),
    (EmployeeColor::Green.as_str(), "Green"),
];
const LIST_ROWS: Options = &[
    (ListRows::Compact.as_str(), "Compact"),
    (ListRows::Comfortable.as_str(), "Comfortable"),
];
const LIST_PREVIEW: Options = &[
    (ListPreview::None.as_str(), "None"),
    (ListPreview::One.as_str(), "1 line"),
    (ListPreview::Two.as_str(), "2 lines"),
];

pub const DISPLAY: Section = section(
    "communications.display",
    "Message display",
    "communications.use",
    &[
        number("text_size", "Text size", 12, 20, Some(14)).unit("px"),
        choice(
            "line_spacing",
            "Line spacing",
            LINE_SPACING,
            LineSpacing::Normal.as_str(),
        ),
        choice("spacing", "Spacing", SPACING, Spacing::Comfortable.as_str()),
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
        choice("style", "Style", STYLE, MessageStyle::Bubbles.as_str()),
        choice(
            "team_side",
            "Your team",
            TEAM_SIDE,
            TeamSide::Right.as_str(),
        ),
        choice(
            "bubble_shape",
            "Bubble shape",
            BUBBLE_SHAPE,
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
            TEAM_COLOR,
            TeamColor::Blue.as_str(),
        ),
        choice(
            "employee_color",
            "Employees' color",
            EMPLOYEE_COLOR,
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
            LIST_ROWS,
            ListRows::Comfortable.as_str(),
        ),
        choice(
            "list_preview",
            "Preview",
            LIST_PREVIEW,
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
        // A phone's own, of the choices that apply on one.
        number("phone_text_size", "Text size on a phone", 12, 20, Some(14)).unit("px"),
        choice(
            "phone_line_spacing",
            "Line spacing on a phone",
            LINE_SPACING,
            LineSpacing::Normal.as_str(),
        ),
        choice(
            "phone_spacing",
            "Spacing on a phone",
            SPACING,
            Spacing::Comfortable.as_str(),
        ),
        choice(
            "phone_style",
            "Style on a phone",
            STYLE,
            MessageStyle::Bubbles.as_str(),
        ),
        choice(
            "phone_team_side",
            "Your team on a phone",
            TEAM_SIDE,
            TeamSide::Right.as_str(),
        ),
        choice(
            "phone_bubble_shape",
            "Bubble shape on a phone",
            BUBBLE_SHAPE,
            BubbleShape::Round.as_str(),
        ),
        number(
            "phone_bubble_width",
            "Bubble width on a phone",
            60,
            100,
            Some(80),
        )
        .unit("%"),
        choice(
            "phone_times",
            "Times on a phone",
            PHONE_TIMES,
            Times::EachGroup.as_str(),
        ),
        toggle("phone_message_photos", "Photos on a phone", true),
        toggle("phone_team_names", "Team names on a phone", true),
        choice(
            "phone_team_color",
            "Your team's color on a phone",
            TEAM_COLOR,
            TeamColor::Blue.as_str(),
        ),
        choice(
            "phone_employee_color",
            "Employees' color on a phone",
            EMPLOYEE_COLOR,
            EmployeeColor::Gray.as_str(),
        ),
        choice(
            "phone_list_rows",
            "Rows on a phone",
            LIST_ROWS,
            ListRows::Comfortable.as_str(),
        ),
        choice(
            "phone_list_preview",
            "Preview on a phone",
            LIST_PREVIEW,
            ListPreview::One.as_str(),
        ),
        toggle("phone_list_photos", "Photos in the list on a phone", true),
        toggle("phone_waiting_time", "Waiting time on a phone", true),
        toggle("phone_profile_slide", "Slide in on a phone", true),
        // A phone's Return starts a line, as in its messaging apps; Send sends.
        toggle("phone_enter_sends", "Enter sends on a phone", false),
    ],
)
.on("display")
.member()
.custom()
.keywords(&[
    "text size",
    "font",
    "bubbles",
    "colors",
    "layout",
    "messages",
    "phone",
]);

#[cfg(test)]
#[path = "../tests/backend/display.rs"]
mod tests;
