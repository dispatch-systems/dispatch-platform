//! HR Line's section of Settings' Display page, each member's own: how its page looks to them and
//! how a new text to its phones alerts them, with Dispatch Phone's choices, apart from Dispatch
//! Phone's own Message display, after it on the page.
use dispatch_communications::lines;
use dispatch_core::manifest::Section;

pub const DISPLAY: Section = lines::display(
    "cjf87gqd_hr_line.display",
    "HR Line",
    "cjf87gqd_hr_line.use",
)
.order(1);
