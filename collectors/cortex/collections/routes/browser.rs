//! A day's routes read outside a job, as the routes collection reads one: for a feature that
//! opens the DSP's Cortex browser only when it must (`readers::Reading::browser`), and reads
//! with the native reader it hands back, with no browser, until that reader fails.
use super::capture::{Request, fixture_in, fixture_scope};
use super::collect::{Day, Method};
use super::native::Native;
use crate::{
    HOSTS,
    connection::{Tab, application},
    discovery::Scope,
};
use dispatch_core::{
    Error, Result,
    collection::{
        browser::{browseros, http::SignedIn, page::Page},
        metrics::Recorder,
    },
    db::now,
    foundation::config::Config,
};
use serde_json::json;

/// The DSP's Cortex browser as a feature opened it outside a job (`ProviderBrowser::browser`),
/// read as the routes collection reads it. In fixture mode no browser runs, and every day it
/// reads is the fixture's, as a job's is.
pub struct Browser {
    /// None in fixture mode.
    application: Option<Application>,
}
/// The application in the browser, as a job's driver holds its own.
struct Application {
    browser: browseros::Session,
    page: Page,
    origin: String,
    origins: Vec<String>,
}
impl Application {
    fn tab(&mut self) -> Tab<'_> {
        Tab {
            browser: &self.browser,
            page: &mut self.page,
            origin: &self.origin,
            origins: &self.origins,
        }
    }
}
/// What a read outside a job records: nothing anyone keeps.
fn unrecorded() -> Recorder {
    Recorder::new(&json!({}))
}
impl Browser {
    /// `browser` as `ProviderBrowser::browser` gives it, in a server with `config`: none only
    /// in fixture mode, where no browser runs. Without one otherwise, as once it has closed,
    /// there is nothing to read with (`browser_unavailable`).
    pub fn new(browser: Option<browseros::Session>, config: &Config) -> Result<Self> {
        let fixture = config.fixture && config.fixture_url.is_none();
        let application = match (browser, fixture) {
            (Some(browser), false) => {
                let (origin, origins) = application(config.fixture_url.as_deref());
                Some(Application {
                    page: Page::empty(browser.clone(), origin.clone()),
                    browser,
                    origin,
                    origins,
                })
            }
            (None, true) => None,
            _ => return Err(Error::new("browser_unavailable", 409)),
        };
        Ok(Self { application })
    }
    /// The scope of the day `request` names: the service area and provider it names, or
    /// found on Cortex as a job finds them.
    pub async fn scope(&mut self, request: &Request) -> Result<Scope> {
        match &mut self.application {
            Some(application) => application.tab().routes_scope(request, &unrecorded()).await,
            None => {
                request.validate()?;
                Ok(fixture_scope(request))
            }
        }
    }
    /// The day `request` names, in `scope`, read as the routes collection reads it: natively
    /// first, then with the application's signatures, then by moves and pages. A day read
    /// natively closes the browser, as a job's does: nothing needs it after. Read another way,
    /// it hands back a reader carrying the browser's session as the read left it, which
    /// signing in again or the application may have changed since the native read began.
    pub async fn read(mut self, request: &Request, scope: &Scope) -> Result<Day> {
        let Some(application) = &mut self.application else {
            return Ok(Day {
                capture: fixture_in(request, scope)?,
                method: None,
                fallback: None,
                native: None,
            });
        };
        let mut day = application
            .tab()
            .read_routes(
                request,
                scope,
                now(),
                &Method::default(),
                &unrecorded(),
                |_, _| async { Ok(()) },
            )
            .await?;
        if day.native.is_none() {
            let session = SignedIn::of(&application.browser, &application.origin, &HOSTS).await?;
            day.native = Some(Native::new(&session)?);
        }
        Ok(day)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/collections/routes/browser.rs"]
mod tests;
