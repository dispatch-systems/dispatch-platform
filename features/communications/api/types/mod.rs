//! What Communications' endpoints answer with, by concern: the list and what changes in it
//! (`inbox`), a conversation of either kind (`conversations`), what only texts have (`texts`), and
//! groups (`groups`). Each type is written to TypeScript in `generated/`, which `client.ts`
//! imports. Times are milliseconds since 1970.
pub use crate::backend::{
    files::FileStatus,
    groups::{GroupChoiceKind, GroupPersonState},
    model::{ConversationKind, ConversationState, Direction},
    texts::model::TextStatus,
    today::Pace,
};

mod conversations;
mod groups;
mod inbox;
mod texts;

pub use conversations::*;
pub use groups::*;
pub use inbox::*;
pub use texts::*;
