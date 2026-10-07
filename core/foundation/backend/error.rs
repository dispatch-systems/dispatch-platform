use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// An error code the backend itself branches on. The wire format stays the code's text;
/// a code that is only ever produced stays a string literal where it is raised. Core's own
/// are listed here; a collector declares its own beside the collections that raise them.
/// Compare with `Error::is`/`Error::is_any`/`Code::text_is_any`, never with the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Code(&'static str);
macro_rules! codes {
    ($($name:ident => $text:literal,)*) => {
        // Core's codes keep the names every comparison of them already reads.
        #[allow(non_upper_case_globals)]
        impl Code {
            $(pub const $name: Code = Code($text);)*
        }
    };
}
codes! {
    BrowserLost => "browser_lost",
    BrowserClosed => "browser_closed",
    BrowserCommandTimeout => "browser_command_timeout",
    BrowserNavigationPending => "browser_navigation_pending",
    BrowserScriptFailed => "browser_script_failed",
    BrowserMemoryBusy => "browser_memory_busy",
    BrowserStartFailed => "browser_start_failed",
    BrowserCapacityBusy => "browser_capacity_busy",
    ProviderTimeout => "provider_timeout",
    ProviderUnavailable => "provider_unavailable",
    ProviderNavigationTimeout => "provider_navigation_timeout",
    ProviderContentTimeout => "provider_content_timeout",
    ProviderContentMissing => "provider_content_missing",
    ProviderHoursMismatch => "provider_hours_mismatch",
    ProviderResponseUnreadable => "provider_response_unreadable",
    QueryLimitExceeded => "query_limit_exceeded",
    VerificationRequired => "verification_required",
    ManualVerificationRequired => "manual_verification_required",
    VerificationIncomplete => "verification_incomplete",
    InvalidVerificationCode => "invalid_verification_code",
    ConnectionBusy => "connection_busy",
    PrimaryCredentialsRejected => "primary_credentials_rejected",
    SecurityAnswersRejected => "security_answers_rejected",
    InvalidCredentials => "invalid_credentials",
    JobCancelled => "job_cancelled",
    PermissionDenied => "permission_denied",
    ConnectionChanged => "connection_changed",
    DspUnavailable => "dsp_unavailable",
    AccountLocked => "account_locked",
}
impl Code {
    /// A collector raises its own codes as these.
    pub const fn new(text: &'static str) -> Self {
        Self(text)
    }
    pub const fn as_str(self) -> &'static str {
        self.0
    }
    /// Core's own codes a failed attempt is queued again after, while attempts remain.
    /// `Code::retryable` adds each registered collector's.
    pub const RETRYABLE: &'static [Code] = &[
        Code::BrowserLost,
        Code::BrowserClosed,
        Code::BrowserCommandTimeout,
        Code::ProviderTimeout,
        Code::ProviderUnavailable,
        Code::ProviderNavigationTimeout,
        Code::ProviderContentTimeout,
        Code::ProviderResponseUnreadable,
    ];
    /// The job stopped because someone withdrew it or its access; that is not a failure.
    pub const WITHDRAWN: &'static [Code] = &[
        Code::JobCancelled,
        Code::PermissionDenied,
        Code::ConnectionChanged,
        Code::DspUnavailable,
    ];
    /// A verification step may be tried again in the same browser after one of these.
    pub const RECOVERABLE: &'static [Code] = &[
        Code::VerificationIncomplete,
        Code::InvalidVerificationCode,
        Code::ConnectionBusy,
    ];
    /// The provider refused what the owner entered; repeated ones lock further attempts.
    pub const REJECTED_CREDENTIALS: &'static [Code] = &[
        Code::PrimaryCredentialsRejected,
        Code::SecurityAnswersRejected,
        Code::InvalidCredentials,
    ];
    /// Only a person at the provider's own site can clear these; signing in again would not.
    pub const NEEDS_OWNER: &'static [Code] =
        &[Code::ManualVerificationRequired, Code::AccountLocked];
    /// No browser could be admitted right now; the job goes back without using an attempt.
    pub const ADMISSION_BUSY: &'static [Code] =
        &[Code::BrowserMemoryBusy, Code::BrowserCapacityBusy];
    /// Classifies a stored or reported code, such as a job's `error` column.
    pub fn text_is_any(text: &str, codes: &[Code]) -> bool {
        codes.iter().any(|code| code.0 == text)
    }
}
impl From<Code> for String {
    fn from(code: Code) -> Self {
        code.as_str().to_owned()
    }
}
impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

type Cause = Box<dyn std::error::Error + Send + Sync + 'static>;
#[derive(Debug)]
pub struct Error {
    pub code: String,
    pub status: u16,
    /// What the operating system or SQLite reported. Never sent to a client.
    cause: Option<Cause>,
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn new(code: impl Into<String>, status: u16) -> Self {
        Self {
            code: code.into(),
            status,
            cause: None,
        }
    }
    pub(crate) fn caused(code: &str, status: u16, cause: impl Into<Cause>) -> Self {
        Self {
            cause: Some(cause.into()),
            ..Self::new(code, status)
        }
    }
    pub fn is(&self, code: Code) -> bool {
        self.code == code.as_str()
    }
    pub fn is_any(&self, codes: &[Code]) -> bool {
        Code::text_is_any(&self.code, codes)
    }
}
pub fn ensure(ok: bool, code: &str, status: u16) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::new(code, status))
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_deref().map(|cause| cause as _)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        super::observability::event(
            "error",
            "storage.io_failed",
            json!({"kind":format!("{:?}",e.kind()),"cause":e.to_string()}),
        );
        Self::caused("operation_failed", 500, e)
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        super::observability::event(
            "error",
            "database.failed",
            json!({"code":e.sqlite_error().map(|e| e.extended_code),"cause":e.to_string()}),
        );
        Self::caused("operation_failed", 500, e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::new("invalid_input", 400)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"error":self.code,"message":self.code.replace('_'," ")})),
        )
            .into_response()
    }
}

#[cfg(test)]
#[path = "../tests/backend/error.rs"]
mod tests;
