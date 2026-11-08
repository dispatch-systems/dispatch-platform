//! Plans feature switches without reading or writing storage.
use super::super::catalog::{Feature, Kind, catalog, provided, satisfied, tabs};
use crate::{Result, ensure};

/// The resulting raw switches and every transition, in cascade order. A feature may occur
/// more than once: each transition is persisted and audited in this order.
#[derive(Debug)]
pub struct Plan {
    pub switches: Vec<String>,
    pub changed: Vec<(&'static Feature, bool)>,
}

/// Plans a validated optional feature's switch against the current raw switches.
pub fn plan(mut current: Vec<String>, feature: &'static Feature, enabled: bool) -> Result<Plan> {
    let all = catalog();
    fn flip(
        current: &mut Vec<String>,
        changed: &mut Vec<(&'static Feature, bool)>,
        f: &'static Feature,
        on: bool,
    ) {
        if current.iter().any(|e| e == f.id) == on {
            return;
        }
        if on {
            current.push(f.id.to_owned());
        } else {
            current.retain(|e| e != f.id);
        }
        changed.push((f, on));
    }
    // On, and with its page on for a part: what exists.
    let live = |current: &[String], f: &Feature| {
        let on = |id: &str| current.iter().any(|e| e == id);
        on(f.id)
            && match f.kind {
                Kind::Sub(page) => on(page),
                _ => true,
            }
    };
    let mut changed: Vec<(&'static Feature, bool)> = Vec::new();
    if enabled {
        for capability in feature.provides {
            for other in all
                .iter()
                .filter(|f| f.provides.contains(capability) && f.id != feature.id)
            {
                flip(&mut current, &mut changed, other, false);
            }
        }
        // Each capability `f` requires that no connection on provides, from its one
        // provider.
        let connect = |current: &mut Vec<String>,
                       changed: &mut Vec<(&'static Feature, bool)>,
                       f: &Feature|
         -> Result<()> {
            for capability in f.requires {
                if provided(capability, current) {
                    continue;
                }
                let providers: Vec<_> = all
                    .iter()
                    .filter(|f| f.provides.contains(capability))
                    .collect();
                ensure(providers.len() == 1, "provider_required", 409)?;
                flip(current, changed, providers[0], true);
            }
            Ok(())
        };
        connect(&mut current, &mut changed, feature)?;
        flip(&mut current, &mut changed, feature, true);
        // A page shows one of its tabs at least: its first, when none is on.
        if !tabs(feature.id).any(|t| current.iter().any(|e| e == t.id))
            && let Some(first) = tabs(feature.id).next()
        {
            flip(&mut current, &mut changed, first, true);
        }
        // A page's parts that come on with it need their connections too.
        let parts: Vec<&'static Feature> = all
            .iter()
            .filter(|f| f.kind == Kind::Sub(feature.id) && live(&current, f))
            .collect();
        for part in parts {
            connect(&mut current, &mut changed, part)?;
        }
    } else {
        flip(&mut current, &mut changed, feature, false);
        // Then whatever is left short, until nothing more is: a page with none of
        // its tabs on, and a page or a part without a provider of what it requires.
        loop {
            let before = changed.len();
            for page in all.iter().filter(|f| f.kind == Kind::Page) {
                let mut own = tabs(page.id).peekable();
                if live(&current, page)
                    && own.peek().is_some()
                    && !own.any(|t| current.iter().any(|e| e == t.id))
                {
                    flip(&mut current, &mut changed, page, false);
                }
            }
            for f in all.iter().filter(|f| f.kind != Kind::Connection) {
                if live(&current, f) && !satisfied(f, &current) {
                    flip(&mut current, &mut changed, f, false);
                }
            }
            if changed.len() == before {
                break;
            }
        }
    }
    Ok(Plan {
        switches: current,
        changed,
    })
}
