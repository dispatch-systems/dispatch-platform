//! What every API's types share: request bodies read into their type, and closed sets of
//! text that are stored in a column and sent as JSON.
use crate::{Error, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;

// `text_enum!` reaches these through `$crate`. Its serde derives still expect the crate
// using it to depend on `serde`.
#[doc(hidden)]
pub use {rusqlite, serde};

pub fn request<T: DeserializeOwned>(value: &Value) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|_| Error::new("invalid_input", 400))
}
pub fn invalid_record() -> Error {
    Error::new("invalid_stored_record", 500)
}

/// A closed set of strings stored in a column and sent as JSON: one enum with its SQL
/// text, `FromSql`/`ToSql`, and serde, so no caller compares the text.
#[macro_export]
macro_rules! text_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($variant:ident => $text:literal,)* }) => {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash,
            $crate::wire::serde::Serialize, $crate::wire::serde::Deserialize,
        )]
        $(#[$meta])*
        $vis enum $name { $(#[serde(rename = $text)] $variant,)* }
        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text,)* }
            }
            pub fn parse(text: &str) -> Option<Self> {
                match text { $($text => Some(Self::$variant),)* _ => None }
            }
        }
        impl $crate::wire::rusqlite::types::FromSql for $name {
            fn column_result(
                value: $crate::wire::rusqlite::types::ValueRef<'_>,
            ) -> $crate::wire::rusqlite::types::FromSqlResult<Self> {
                value.as_str().and_then(|text| {
                    Self::parse(text).ok_or($crate::wire::rusqlite::types::FromSqlError::InvalidType)
                })
            }
        }
        impl $crate::wire::rusqlite::types::ToSql for $name {
            fn to_sql(
                &self,
            ) -> $crate::wire::rusqlite::Result<$crate::wire::rusqlite::types::ToSqlOutput<'_>> {
                Ok(self.as_str().into())
            }
        }
    };
}
