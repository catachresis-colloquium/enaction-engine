// SPDX-License-Identifier: AGPL-3.0-or-later
//! Models are compared with minds, not with the world (CAC-KERNEL §9,
//! "Models are compared with minds"; ADR-0028).
//!
//! A belief of order 1 or more is a *model*: `holders = ["guard-0"]`,
//! `bearer = "contact-21"` is guard-0's model of what contact-21 believes.
//! World truth applies to a belief of order 0 only. A model is compared
//! with the modelled bearer's own belief of one order less, about the same
//! proposition, in the same package, over the model's whole lifetime.
//!
//! This module derives the comparison. It reduces nothing (ADR-0019): it
//! reads a validated package and reports, for each model, a *timeline* of
//! verdicts ([`ModelTimeline`]) and whether the model went stale
//! ([`stale_divergence`]). The producer asserts neither, and the package
//! has no member for them — the same discipline as
//! [`crate::mind::commitment_override`].
//!
//! The functions take a package and nothing else. They have no world truth
//! to consult, so a model cannot be compared with world truth by
//! construction.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::belief::{Belief, BeliefId, BeliefStatus, Proposition};
use crate::package::EpistemicPackage;

/// Every rule name a [`ModelFault`] can carry.
pub const MODEL_RULES: &[&str] = &["model-counterpart-unique"];

/// What a reader derives for one model over one stretch of ticks.
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

/// One stretch of a model's timeline: the verdict from `from_tick` until the
/// next segment starts, or until the model's lifetime ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub from_tick: u64,
    pub verdict: ModelVerdict,
}

/// The verdicts of one model over its lifetime.
///
/// The lifetime starts at the model's `valid_from` and ends at `until`, the
/// earliest `valid_from` of a belief that supersedes the model. A model that
/// nothing supersedes holds to the end of the package, and `until` is
/// `None`. `segments` is never empty; the first starts at the model's
/// `valid_from`, and consecutive segments always differ in verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelTimeline {
    pub model: BeliefId,
    pub segments: Vec<Segment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
}

/// Whether a model is *stale-divergent*: it agreed when it was stated and
/// diverges later in its lifetime. This is the checkable form of a false
/// exposure that the agent never revised.
#[must_use]
pub fn stale_divergence(timeline: &ModelTimeline) -> bool {
    match timeline.segments.split_first() {
        Some((first, rest)) => {
            first.verdict == ModelVerdict::Agrees
                && rest.iter().any(|s| s.verdict == ModelVerdict::Diverges)
        }
        None => false,
    }
}

/// The package rules cannot decide which belief a model is compared with:
/// at `tick`, inside the model's lifetime, more than one belief of the
/// compared kind holds, and none supersedes the others. The reader refuses
/// the model rather than guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFault {
    /// The model that cannot be compared.
    pub model: BeliefId,
    /// The first tick of its lifetime at which the comparison is undecided.
    pub tick: u64,
    /// The beliefs that all hold at `tick`, by id.
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
            "beliefs {} all hold at tick {} in the lifetime of model {} and none \
             supersedes the others",
            ids.join(", "),
            self.tick,
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

/// The timeline of every model in a package, in belief-id order.
///
/// For each belief of order 1 or more (a *model*):
///
/// * its lifetime runs from its `valid_from` to `until`, the earliest
///   `valid_from` of a belief that supersedes it, or to the end of the
///   package when nothing does;
/// * status `unknown` gives one [`ModelVerdict::NoClaim`] segment; nothing
///   is compared;
/// * otherwise the model claims that the bearer holds the proposition
///   (status `certain`, `believed` or `suspected`) or does not (`rejected`).
///   At each tick it is compared with the *counterpart*: the belief with
///   path [`compared_path`], the same bearer and the same proposition, that
///   holds at that tick;
/// * a belief holds at tick `t` when its `valid_from <= t` and no belief in
///   the package that supersedes it has a `valid_from <= t`;
/// * no counterpart holds at `t`: the bearer does not hold the proposition
///   (the package is closed over stated models);
/// * one holds: the bearer holds the proposition iff that belief's status
///   holds it;
/// * the verdict is evaluated at the model's `valid_from` and at every tick
///   strictly inside the lifetime where a belief of the compared kind starts
///   or stops holding. A segment starts where the verdict changes;
/// * more than one counterpart holds at any of those ticks: the model is
///   refused as `model-counterpart-unique`.
///
/// World truth is never consulted. Order-0 beliefs get no timeline.
///
/// The package is expected to be valid ([`crate::validate_package`]); the
/// derivation does not re-check it, and on an invalid package its result
/// means nothing (it still terminates).
///
/// # Errors
///
/// Every model whose counterpart the package rules cannot decide, in
/// belief-id order.
pub fn model_timelines(package: &EpistemicPackage) -> Result<Vec<ModelTimeline>, Vec<ModelFault>> {
    // The earliest tick at which each belief is superseded.
    let mut superseded_from: BTreeMap<BeliefId, u64> = BTreeMap::new();
    for belief in &package.beliefs {
        if let Some(link) = belief.supersedes {
            superseded_from
                .entry(link.belief)
                .and_modify(|t| *t = (*t).min(belief.valid_from))
                .or_insert(belief.valid_from);
        }
    }
    let ends = |belief: &Belief| superseded_from.get(&belief.belief_id).copied();
    let holds_at = |belief: &Belief, tick: u64| {
        belief.valid_from <= tick && ends(belief).is_none_or(|from| from > tick)
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

    let mut timelines = Vec::new();
    let mut faults = Vec::new();
    'models: for model in models {
        let Some(path) = compared_path(model) else {
            continue;
        };
        let start = model.valid_from;
        let until = ends(model);
        if model.status == BeliefStatus::Unknown {
            timelines.push(ModelTimeline {
                model: model.belief_id,
                segments: vec![Segment {
                    from_tick: start,
                    verdict: ModelVerdict::NoClaim,
                }],
                until,
            });
            continue;
        }
        let kind: &[&Belief] = by_kind
            .get(&(path, model.bearer.as_str(), &model.proposition))
            .map_or(&[], Vec::as_slice);
        // The ticks at which the counterpart can change: where a belief of
        // the compared kind starts or stops holding, strictly inside the
        // lifetime.
        let inside = |t: u64| t > start && until.is_none_or(|u| t < u);
        let mut ticks: BTreeSet<u64> = BTreeSet::from([start]);
        for belief in kind {
            ticks.extend(
                std::iter::once(belief.valid_from)
                    .chain(ends(belief))
                    .filter(|&t| inside(t)),
            );
        }
        let claims = holds(model.status);
        let mut segments: Vec<Segment> = Vec::new();
        for tick in ticks {
            let mut current: Vec<&Belief> =
                kind.iter().copied().filter(|b| holds_at(b, tick)).collect();
            current.sort_by_key(|b| b.belief_id);
            let bearer_holds = match current.as_slice() {
                [] => false,
                [one] => holds(one.status),
                many => {
                    faults.push(ModelFault {
                        model: model.belief_id,
                        tick,
                        candidates: many.iter().map(|b| b.belief_id).collect(),
                    });
                    continue 'models;
                }
            };
            let verdict = if claims == bearer_holds {
                ModelVerdict::Agrees
            } else {
                ModelVerdict::Diverges
            };
            if segments.last().is_none_or(|s| s.verdict != verdict) {
                segments.push(Segment {
                    from_tick: tick,
                    verdict,
                });
            }
        }
        timelines.push(ModelTimeline {
            model: model.belief_id,
            segments,
            until,
        });
    }

    if faults.is_empty() {
        Ok(timelines)
    } else {
        Err(faults)
    }
}
