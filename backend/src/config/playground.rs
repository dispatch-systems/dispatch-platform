use crate::{Error, Result, ensure};

#[derive(Clone)]
pub struct Playground {
    pub origin: String,
    pub key: String,
}

impl Playground {
    pub fn load(development: bool) -> Result<Option<Self>> {
        let origin = std::env::var("DISPATCH_PLAYGROUND_ORIGIN").ok();
        let file = std::env::var("DISPATCH_PLAYGROUND_KEY_FILE").ok();
        let (origin, file) = match (origin, file) {
            (None, None) => return Ok(None),
            (Some(origin), Some(file)) => (origin, file),
            _ => return Err(Error::new("playground_configuration_required", 400)),
        };
        let url =
            url::Url::parse(&origin).map_err(|_| Error::new("invalid_playground_origin", 400))?;
        ensure(
            url.origin().ascii_serialization() == origin
                && url.username().is_empty()
                && url.password().is_none()
                && (url.scheme() == "https" || (development && url.scheme() == "http")),
            "invalid_playground_origin",
            400,
        )?;
        let key = std::fs::read_to_string(file)
            .map_err(|_| Error::new("playground_configuration_required", 400))?
            .trim()
            .to_owned();
        ensure(
            key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid_playground_key",
            400,
        )?;
        Ok(Some(Self { origin, key }))
    }
}
