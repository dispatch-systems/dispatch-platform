//! The integrations slot: what a feature does through another feature's connection, which the
//! connection's settings switch on and off for each of its accounts, as Documents saves the
//! files posted in a DSP's Slack channels.
//!
//! The connection's owner reads them with `Registry::integrations`. It asks its service for
//! what they need, keeps which are on for each of its accounts, and hands each what it hears
//! of while it's on there and its feature is on for the DSP. The integration's feature keeps
//! everything else it does, and its frontend says what it is in the connection's settings.
use crate::{
    Result, State,
    manifest::{Feature, Registry, Upkeep},
};
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

/// One feature's integration with a connection: another feature's, which it depends on, or
/// its own, as a connection's feature may offer integrations of its own.
pub struct Integration {
    /// The connection it works through, by the name of the feature that owns it.
    pub connection: &'static str,
    /// Permanent once shipped, as the connection keeps it for each account:
    /// `<feature>.<what>`.
    pub id: &'static str,
    /// How the activity log names it.
    pub label: &'static str,
    /// What it needs the service to let Dispatch do, in the service's own terms, as Slack's
    /// permissions: the connection asks for them whether it's on or not.
    pub needs: &'static [&'static str],
    /// What it's told of, in the service's terms, as Slack's events.
    pub hears: &'static [&'static str],
    /// What it does with each, once the connection's owner has answered the service.
    pub heard: Option<fn(Arc<State>, Heard) -> Upkeep>,
    /// Fixture mode's answer to a method of the service's that only it calls, for the account
    /// named first: the connection's fixture asks it of each method it doesn't answer itself,
    /// so previews and tests never reach the service. None for one it doesn't call.
    pub fixture: Option<Fixture>,
}

/// Fixture mode's answer to a method, for an account: none for a method it doesn't call.
pub type Fixture = fn(account: &str, method: &str, args: &Value) -> Option<Result<Value>>;

/// An integration that needs nothing and hears of nothing yet. A declaration starts here and
/// names what it adds: `Integration { …, ..integration("slack", "x.what", "What") }`.
pub const fn integration(
    connection: &'static str,
    id: &'static str,
    label: &'static str,
) -> Integration {
    Integration {
        connection,
        id,
        label,
        needs: &[],
        hears: &[],
        heard: None,
        fixture: None,
    }
}

/// Something an integration hears of: what the service sent, from which of the DSP's accounts.
pub struct Heard {
    pub dsp: String,
    /// The account it came from, as the connection's owner names it: a Slack workspace's ID.
    pub account: String,
    /// What it is, in the service's terms.
    pub kind: String,
    pub event: Value,
}

/// Panics unless every integration is as the slot asks, as the registry is installed: its id
/// its own and under its feature's name, its connection its own or a feature it depends on, and
/// what it does with what it hears declared exactly when it hears of something.
pub(super) fn check(features: &[&'static Feature]) {
    let mut ids = BTreeSet::new();
    for feature in features {
        for integration in feature.integrations {
            let id = integration.id;
            assert!(
                id.strip_prefix(feature.name)
                    .and_then(|rest| rest.strip_prefix('.'))
                    .is_some_and(|what| !what.is_empty()),
                "{}'s integration {id} isn't named <feature>.<what>",
                feature.name
            );
            assert!(ids.insert(id), "the integration {id} is declared twice");
            assert!(
                integration.connection == feature.name
                    || feature.depends_on.contains(&integration.connection),
                "{}'s integration {id} works through {}, which it doesn't depend on",
                feature.name,
                integration.connection
            );
            assert!(
                integration.hears.is_empty() == integration.heard.is_none(),
                "{}'s integration {id} must say what it does with what it hears, and hear of \
                 something if it does",
                feature.name
            );
        }
    }
}

impl Registry {
    /// Every feature's integrations through the connection `connection`, each with the
    /// feature it's of, in the registry's order.
    pub fn integrations(
        &self,
        connection: &str,
    ) -> impl Iterator<Item = (&'static Feature, &'static Integration)> {
        self.features.iter().flat_map(move |feature| {
            feature
                .integrations
                .iter()
                .filter(move |integration| integration.connection == connection)
                .map(move |integration| (*feature, integration))
        })
    }
}
