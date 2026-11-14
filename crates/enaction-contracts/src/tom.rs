// SPDX-License-Identifier: AGPL-3.0-or-later
//! Models are compared with minds, not with the world (CAC-KERNEL §9,
//! "Models are compared with minds"; ADR-0028).
//!
//! A belief of order 1 or more is a *model*: `holders = ["guard-0"]`,
//! `bearer = "contact-21"` is guard-0's model of what contact-21 believes.
//! World truth applies to a belief of order 0 only. A model is compared
//! with the modelled bearer's own belief of one order less, about the same
//! proposition, in the same package.
//!
//! This module derives the comparison. It reduces nothing (ADR-0019): it
//! reads a validated package and reports, for each model, whether it
//! [agrees](ModelVerdict::Agrees), [diverges](ModelVerdict::Diverges) or
//! makes [no claim](ModelVerdict::NoClaim). The producer does not assert the
//! verdict and the package has no member for it — the same discipline as
//! [`crate::mind::commitment_override`].
//!
//! The function takes a package and nothing else. It has no world truth to
//! consult, so a model cannot be compared with world truth by construction.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::belief::{Belief, BeliefId, BeliefStatus, Proposition};
use crate::package::EpistemicPackage;

/// Every rule name a [`ModelFault`] can carry.
pub const MODEL_RULES: &[&str] = &["model-counterpart-unique"];

/// What a reader derives for one model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelVerdict {
    /// The model and the modelled belief both hold the proposition, or
    /// neither holds it.
    Agrees,
    /// One holds the proposition and the other does not.
    Diverges,
    /// The model has status `unknown`: it says nothing about whether the
    /// bearer holds the proposition.
    NoClaim,
}

/// The package rules cannot decide which belief a model is compared with:
/// more than one belief of the compared kind holds at the model's
/// `valid_from`, and none supersedes the others. The reader refuses rather
/// than guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFault {
    /// The model that cannot be compared.
    pub model: BeliefId,
    /// The beliefs that all hold at the model's `valid_from`, by id.
    pub candidates: Vec<BeliefId>,
}

impl ModelFault {
    /// The rule that failed, by stable name.
    #[must_use]
    pub fn rule(&self) -> &'static str {
        "model-counterpart-unique"
    }

    /// The stable field path of the model, in the package's path form.
    #[must_use]
    pub fn path(&self) -> String {
        format!("beliefs[belief_id={}]", self.model.0)
    }
}

impl std::fmt::Display for ModelFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<String> = self.candidates.iter().map(|c| c.0.to_string()).collect();
        write!(
            f,
            "beliefs {} all hold at the valid_from of model {} and none supersedes the others",
            ids.join(", "),
            self.model.0
        )
    }
}

impl std::error::Error for ModelFault {}

/// Whether a status holds the proposition: `certain`, `believed` or
/// `suspected`. `rejected` and `unknown` do not.
#[must_use]
pub fn holds(status: BeliefStatus) -> bool {
    matches!(
        status,
        BeliefStatus::Certain | BeliefStatus::Believed | BeliefStatus::Suspected
    )
}

/// The ascription path of the belief that a model is compared with: the
/// model's path with its FIRST (outermost) holder removed. For
/// `holders = [h1, …, hn]` that is `[h2, …, hn]`, with the same bearer. For
/// order 1 it is empty: the bearer's own belief.
///
/// This is the reading of `enaction_trace::Trope::is_ascription_of`
/// (`self.holders[1..] == other.holders[..]`). `None` for order 0, which
/// models nothing.
#[must_use]
pub fn compared_path(model: &Belief) -> Option<&[String]> {
    model.holders.split_first().map(|(_, rest)| rest)
}

/// The key of a belief's kind: ascription path, bearer, proposition.
type Kind<'a> = (&'a [String], &'a str, &'a Proposition);

/// The derived verdict for each model in a package, in belief-id order.
///
/// For each belief of order 1 or more (a *model*):
///
/// * status `unknown` gives [`ModelVerdict::NoClaim`]; nothing is compared;
/// * otherwise the model claims that the bearer holds the proposition
///   (status `certain`, `believed` or `suspected`) or does not (`rejected`),
///   and is compared with the *counterpart*: the belief with path
///   [`compared_path`], the same bearer and the same proposition, that
///   holds at the model's `valid_from`;
/// * a belief holds at tick `t` when its `valid_from <= t` and no belief in
///   the package that supersedes it has a `valid_from <= t`;
/// * no counterpart holds at `t`: the bearer does not hold the proposition
///   (the package is closed over stated models);
/// * one holds: the bearer holds the proposition iff that belief's status
///   holds it;
/// * more than one holds: refused as `model-counterpart-unique`.
///
/// World truth is never consulted. Order-0 beliefs get no verdict here.
///
/// The package is expected to be valid ([`crate::validate_package`]); the
/// derivation does not re-check it, and on an invalid package its result
/// means nothing (it still terminates).
///
/// # Errors
///
/// Every model whose counterpart the package rules cannot decide, in
/// belief-id order.
pub fn model_agreements(
    package: &EpistemicPackage,
) -> Result<Vec<(BeliefId, ModelVerdict)>, Vec<ModelFault>> {
    // The earliest tick at which each belief is superseded.
    let mut superseded_from: BTreeMap<BeliefId, u64> = BTreeMap::new();
    for belief in &package.beliefs {
        if let Some(target) = belief.supersedes {
            superseded_from
                .entry(target)
                .and_modify(|t| *t = (*t).min(belief.valid_from))
                .or_insert(belief.valid_from);
        }
    }
    let holds_at = |belief: &Belief, tick: u64| {
        belief.valid_from <= tick
            && superseded_from
                .get(&belief.belief_id)
                .is_none_or(|&from| from > tick)
    };

    let mut by_kind: BTreeMap<Kind<'_>, Vec<&Belief>> = BTreeMap::new();
    for belief in &package.beliefs {
        by_kind
            .entry((
                belief.holders.as_slice(),
                belief.bearer.as_str(),
                &belief.proposition,
            ))
            .or_default()
            .push(belief);
    }

    let mut models: Vec<&Belief> = package.beliefs.iter().filter(|b| b.order() >= 1).collect();
    models.sort_by_key(|b| b.belief_id);

    let mut verdicts = Vec::new();
    let mut faults = Vec::new();
    for model in models {
        let Some(path) = compared_path(model) else {
            continue;
        };
        if model.status == BeliefStatus::Unknown {
            verdicts.push((model.belief_id, ModelVerdict::NoClaim));
            continue;
        }
        let mut current: Vec<&Belief> = by_kind
            .get(&(path, model.bearer.as_str(), &model.proposition))
            .map(|kind| {
                kind.iter()
                    .copied()
                    .filter(|b| holds_at(b, model.valid_from))
                    .collect()
            })
            .unwrap_or_default();
        current.sort_by_key(|b| b.belief_id);
        let bearer_holds = match current.as_slice() {
            [] => false,
            [one] => holds(one.status),
            many => {
                faults.push(ModelFault {
                    model: model.belief_id,
                    candidates: many.iter().map(|b| b.belief_id).collect(),
                });
                continue;
            }
        };
        let verdict = if holds(model.status) == bearer_holds {
            ModelVerdict::Agrees
        } else {
            ModelVerdict::Diverges
        };
        verdicts.push((model.belief_id, verdict));
    }

    if faults.is_empty() {
        Ok(verdicts)
    } else {
        Err(faults)
    }
}
