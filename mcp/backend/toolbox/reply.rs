//! What a tool answers: its data, with words and pictures beside it where they help; or why
//! there is no answer, which the agent can fix or which is a failure of ours.
use dispatch_core::Error;
use serde::Serialize;
use serde_json::Value;

/// A tool's answer: its data, which the server sends as structured content and as its JSON
/// text, a few words beside it for the model or the user, and pictures, such as a chart.
/// `Ok(data.into())` answers with the data alone.
pub struct Reply<T> {
    pub data: T,
    pub text: Option<String>,
    pub images: Vec<Image>,
}
impl<T> Reply<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            text: None,
            images: vec![],
        }
    }
    /// Words sent after the data, such as a summary to read out.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
    /// A picture sent after the data and its words.
    pub fn image(mut self, image: Image) -> Self {
        self.images.push(image);
        self
    }
}
impl<T> From<T> for Reply<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}
impl<T: Serialize> Reply<T> {
    /// The reply as the server sends it.
    pub(super) fn answered(self) -> Answer<Answered> {
        Ok(Answered {
            data: serde_json::to_value(self.data)
                .map_err(|_| Failure::Failed(Error::new("invalid_answer", 500)))?,
            text: self.text,
            images: self.images,
        })
    }
}

/// A picture in a reply: its bytes, and their type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub mime: ImageType,
    pub bytes: Vec<u8>,
}
/// The kinds of picture every MCP host shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageType {
    Png,
    Jpeg,
    Webp,
}
impl ImageType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }
}

/// A reply as the server sends it, whatever the tool's types.
#[derive(Debug, PartialEq)]
pub struct Answered {
    pub data: Value,
    pub text: Option<String>,
    pub images: Vec<Image>,
}

/// Why a tool gave no answer: something the agent can fix or ask the user about, or a
/// failure of ours.
#[derive(Debug)]
pub enum Failure {
    Refused(Refusal),
    Failed(Error),
}
/// A call the agent can fix, in words it can repeat: what was wrong, and what it could have
/// meant.
#[derive(Debug)]
pub struct Refusal {
    pub code: String,
    pub message: String,
    pub choices: Vec<String>,
}
impl Refusal {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            choices: vec![],
        }
    }
    pub fn choices(mut self, choices: Vec<String>) -> Self {
        self.choices = choices;
        self
    }
}
impl From<Refusal> for Failure {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}
/// An error Dispatch refuses a request with, as a 404 or a 409, is one the agent can act on;
/// any other is a failure of ours.
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        if error.status < 500 {
            let message = "Dispatch refused the request.";
            Self::Refused(Refusal::new(&error.code, message))
        } else {
            Self::Failed(error)
        }
    }
}
/// A tool's answer, or why there is none.
pub type Answer<T> = std::result::Result<T, Failure>;
