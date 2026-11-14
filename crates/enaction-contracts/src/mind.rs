// SPDX-License-Identifier: AGPL-3.0-or-later
//! The mind trace `enaction-mind-trace/1` (CAC-KERNEL §10;
//! WEEVIL-HOST-ROUTE, "The mind trace contract").
//!
//! A mind trace records, per agent and tick, the affect token and the
//! attention it buys, the conative *ranking* and the selected candidate, and
//! the bodily tells. It is a separate document from
//! `enaction-runner-output/1`, so that the strict envelope gains no optional
//! member. It is validated against a paired runner output, whose package
//! events are the only causes an affect change may cite.
//!
//! The document carries no utility score and no float: a ranking is
//! published, never a score (ADR-0016), and the canonical decoder refuses
//! every non-integer number (ADR-0024).
//!
//! Commitment override is derived here ([`commitment_override`]), never
//! asserted by the producer: the document has no member for it.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::Mass;
use crate::canonical::{self, Value};
use crate::envelope::{Fault, RunnerOutput, check_contract};

/// The contract tag of the mind trace.
pub const MIND_TRACE_CONTRACT: &str = "enaction-mind-trace/1";

/// Every rule the mind-trace validator names, as the contract lists them.
/// The canonical decoder can also report `MalformedJson`, `DuplicateKey`,
/// `NonIntegerNumber` and `BadShape` for a document it cannot read at all.
pub const MIND_RULES: &[&str] = &[
    "UnknownField",
    "UndeclaredTerm",
    "BadVocabulary",
    "NonMonotonicTick",
    "BadRanking",
    "UnexplainedSelection",
    "MassOutOfRange",
    "UncausedAffectChange",
    "DanglingCause",
    "IntegerOutOfRange",
];

/// The whole document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MindTrace {
    /// Always [`MIND_TRACE_CONTRACT`].
    pub contract: String,
    pub producer: Producer,
    pub profile: MindProfile,
    pub vocabulary: Vocabulary,
    pub ticks: Vec<MindTick>,
}

/// The kernel that produced the trace, e.g. `evil-weevil` at ABI `1.1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Producer {
    pub kernel: String,
    pub abi: String,
}

/// The game profile the trace was produced under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MindProfile {
    pub id: String,
    pub version: String,
}

/// The declared terms. Each list is nonempty, without duplicates, and in
/// declaration order (which the producer's tie-breaks use).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vocabulary {
    pub affect_tokens: Vec<String>,
    pub candidates: Vec<String>,
    pub tells: Vec<String>,
}

/// One agent at one tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MindTick {
    pub tick: u64,
    pub agent: String,
    pub affect: Affect,
    pub conation: Conation,
    /// Bodily tells this tick, from the declared vocabulary.
    pub tells: Vec<String>,
}

/// The affect token and what it buys. There is no member on which affect
/// could assert a proposition: affect buys attention, never belief.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Affect {
    pub token: String,
    /// Attention, in mass units `0 ..= 10_000`, if the host publishes it
    /// (ADR-0027: optional). Read as a plain integer so that a value above
    /// full mass is refused with its own rule, `MassOutOfRange`, rather than
    /// as a shape fault.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<u64>,
    /// The runner-output event ids that caused this tick's token. Required
    /// to be nonempty when the token changed.
    pub caused_by: Vec<u64>,
}

/// The conative layer's published decision: a ranking and a selection,
/// never a score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conation {
    /// Candidates, best first.
    pub ranking: Vec<String>,
    /// The candidate the agent is already committed to (ADR-0027: required).
    pub incumbent: String,
    pub selected: String,
    /// What the selection is aimed at, if anything. Opaque to the kernel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Whether a tick shows commitment override: the selected candidate is the
/// incumbent and is not first in the published ranking. The agent acted from
/// commitment, not from the ranking.
#[must_use]
pub fn commitment_override(tick: &MindTick) -> bool {
    let c = &tick.conation;
    c.incumbent == c.selected && c.ranking.first().map(String::as_str) != Some(c.selected.as_str())
}

/// The `(agent, tick)` of every commitment-override tick, in document order.
#[must_use]
pub fn commitment_overrides(trace: &MindTrace) -> Vec<(String, u64)> {
    trace
        .ticks
        .iter()
        .filter(|t| commitment_override(t))
        .map(|t| (t.agent.clone(), t.tick))
        .collect()
}

fn has_duplicate(items: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    !items.iter().all(|i| seen.insert(i))
}

/// Parse a mind trace strictly and validate it against its paired runner
/// output. On success returns the trace and its parsed value, whose
/// [`canonical::to_jcs`] bytes are the trace's canonical bytes.
///
/// # Errors
///
/// Every fault found. A document that cannot be read yields one fault.
pub fn validate_mind_trace(
    bytes: &[u8],
    runner: &RunnerOutput,
) -> Result<(MindTrace, Value), Vec<Fault>> {
    let value = canonical::parse(bytes).map_err(|f| vec![f.into()])?;
    let trace: MindTrace = canonical::decode_value(&value).map_err(|f| vec![f.into()])?;
    let mut faults = Vec::new();
    if let Some(fault) = check_contract(&trace.contract, MIND_TRACE_CONTRACT) {
        faults.push(fault);
    }

    let vocab = &trace.vocabulary;
    for (name, list) in [
        ("affect_tokens", &vocab.affect_tokens),
        ("candidates", &vocab.candidates),
        ("tells", &vocab.tells),
    ] {
        if list.is_empty() || has_duplicate(list) {
            faults.push(Fault::new(
                format!("vocabulary.{name}"),
                "BadVocabulary",
                "a vocabulary is empty or has a duplicate",
            ));
        }
    }
    let declared = |list: &[String], term: &str| list.iter().any(|t| t == term);

    let events: BTreeSet<u64> = runner.package.events.iter().map(|e| e.event_id.0).collect();
    let full = u64::from(Mass::SCALE);
    // Per agent: the last tick and its affect token.
    let mut last: BTreeMap<&str, (u64, &str)> = BTreeMap::new();

    for (i, t) in trace.ticks.iter().enumerate() {
        let at = |field: &str| format!("ticks[{i}].{field}");
        let mut undeclared = |path: String, kind: &str, term: &str| {
            faults.push(Fault::new(
                path,
                "UndeclaredTerm",
                format!("{kind} {term:?} is not declared"),
            ));
        };

        // Vocabulary membership.
        if !declared(&vocab.affect_tokens, &t.affect.token) {
            undeclared(at("affect.token"), "affect token", &t.affect.token);
        }
        for (j, candidate) in t.conation.ranking.iter().enumerate() {
            if !declared(&vocab.candidates, candidate) {
                undeclared(
                    at(&format!("conation.ranking[{j}]")),
                    "candidate",
                    candidate,
                );
            }
        }
        if !declared(&vocab.candidates, &t.conation.incumbent) {
            undeclared(at("conation.incumbent"), "candidate", &t.conation.incumbent);
        }
        if !declared(&vocab.candidates, &t.conation.selected) {
            undeclared(at("conation.selected"), "candidate", &t.conation.selected);
        }
        for (j, tell) in t.tells.iter().enumerate() {
            if !declared(&vocab.tells, tell) {
                undeclared(at(&format!("tells[{j}]")), "tell", tell);
            }
        }

        // Ranking and selection.
        let c = &t.conation;
        if c.ranking.is_empty() || has_duplicate(&c.ranking) {
            faults.push(Fault::new(
                at("conation.ranking"),
                "BadRanking",
                "a ranking is empty or has a duplicate",
            ));
        } else if !c.ranking.contains(&c.selected) {
            faults.push(Fault::new(
                at("conation.selected"),
                "BadRanking",
                format!("selected {:?} is not in the ranking", c.selected),
            ));
        } else if c.ranking[0] != c.selected && c.incumbent != c.selected {
            faults.push(Fault::new(
                at("conation.selected"),
                "UnexplainedSelection",
                format!(
                    "selected {:?} is neither first in the ranking nor the incumbent",
                    c.selected
                ),
            ));
        }

        // Attention is a mass.
        if let Some(attention) = t.affect.attention
            && attention > full
        {
            faults.push(Fault::new(
                at("affect.attention"),
                "MassOutOfRange",
                format!("attention {attention} is above full mass {full}"),
            ));
        }

        // Causes exist in the paired runner output.
        for (j, cause) in t.affect.caused_by.iter().enumerate() {
            if !events.contains(cause) {
                faults.push(Fault::new(
                    at(&format!("affect.caused_by[{j}]")),
                    "DanglingCause",
                    format!("event {cause} is not in the paired runner output"),
                ));
            }
        }

        // Per agent: strictly increasing ticks, and every change of affect
        // token has a cause.
        if let Some(&(prev_tick, prev_token)) = last.get(t.agent.as_str()) {
            if t.tick <= prev_tick {
                faults.push(Fault::new(
                    at("tick"),
                    "NonMonotonicTick",
                    format!(
                        "tick {} for {:?} does not follow tick {prev_tick}",
                        t.tick, t.agent
                    ),
                ));
            }
            if t.affect.token != prev_token && t.affect.caused_by.is_empty() {
                faults.push(Fault::new(
                    at("affect.caused_by"),
                    "UncausedAffectChange",
                    format!(
                        "affect {prev_token:?} -> {:?} with no cause",
                        t.affect.token
                    ),
                ));
            }
        }
        last.insert(t.agent.as_str(), (t.tick, t.affect.token.as_str()));
    }

    if faults.is_empty() {
        Ok((trace, value))
    } else {
        Err(faults)
    }
}
