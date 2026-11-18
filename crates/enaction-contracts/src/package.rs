// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package declaration and validation (CAC-KERNEL §6).
//!
//! A package is a *compiled artifact*, and that difference from a live trace
//! is deliberate policy here: `enaction_trace::Trace::validate` tolerates
//! unsorted events because causality is judged by stamp, but §6 puts
//! non-monotonic ordering on the loader's reject list — a compiler had every
//! opportunity to sort, so arriving unsorted is evidence of a broken producer,
//! not a lenient caller.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::belief::{Belief, BeliefId, BeliefStatus, EpistemicEvent, EventId, Supersession};
use crate::canonical::{self, CanonicalFault, Value};
use crate::version::{ContractVersion, ProfileRef};
use crate::{DepthPolicy, StatusPolicy};

/// How a package obtains its deterministic seed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedPolicy {
    /// A literal seed, fixed at compile time.
    Fixed(u64),
    /// A named, versioned policy the host resolves. The kernel never
    /// interprets the name.
    Policy(String),
}

/// Everything a package MUST declare (CAC-KERNEL §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub kernel_version: ContractVersion,
    pub profile: ProfileRef,
    pub schema_version: ContractVersion,
    /// Source provenance: where this package was compiled from.
    pub source: String,
    /// Content digest: lowercase hex SHA-256 over the canonical bytes
    /// (RFC 8785 JCS, ADR-0024) of the package with this one field omitted
    /// — a digest cannot hash itself. [`validate_package`] computes it from
    /// the package and compares; no caller can vouch for a mutated package.
    /// See [`package_digest`].
    pub digest: String,
    /// The oldest declared version this package can migrate from.
    pub migration_from: ContractVersion,
    pub seed: SeedPolicy,
    /// Declared event and snapshot guarantees, as namespaced strings. Two
    /// real packages need to exist before a typed vocabulary would be more
    /// than guesswork.
    pub guarantees: Vec<String>,
}

/// A validated unit of epistemic content: manifest, events, beliefs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpistemicPackage {
    pub manifest: PackageManifest,
    pub events: Vec<EpistemicEvent>,
    pub beliefs: Vec<Belief>,
}

/// One reason a loader MUST reject a package (CAC-KERNEL §6).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageFault {
    IncompatibleKernel {
        declared: ContractVersion,
        host: ContractVersion,
    },
    DuplicateEventId(EventId),
    DuplicateBeliefId(BeliefId),
    NonMonotonicOrder {
        at: EventId,
        prev: (u64, u64),
        this: (u64, u64),
    },
    NestingBeyondDeclaredMax {
        belief: BeliefId,
        order: usize,
        max: usize,
    },
    /// A non-derived belief with no provenance events at all.
    MissingProvenance(BeliefId),
    /// A belief citing an event the package does not contain.
    UnknownSourceEvent {
        belief: BeliefId,
        event: EventId,
    },
    SupersedesUnknownBelief {
        belief: BeliefId,
        superseded: BeliefId,
    },
    /// Supersession chains must not loop: contradiction history is a line,
    /// not a circle.
    SupersessionCycle(BeliefId),
    /// The beliefs that share one key (`holders`, `bearer`, `proposition`)
    /// do not form one chain under `supersedes` (CAC-KERNEL §3 invariant 4,
    /// ADR-0029). `superseded: None` is a silent overwrite: `belief` is a
    /// later belief of the key that supersedes no belief of the key.
    /// `superseded: Some(t)` is a fork: `belief` supersedes `t`, which
    /// another belief of the key already supersedes.
    BeliefKeyNotChained {
        belief: BeliefId,
        superseded: Option<BeliefId>,
    },
    /// The supersession link names, as its cause, an event the package does
    /// not contain (ADR-0030). The other cause rules are not evaluated for
    /// this link: there is no event to test.
    SupersessionCauseUnknown {
        belief: BeliefId,
        cause: EventId,
    },
    /// The cause did not reach the mind that revised. The revising mind is
    /// the outermost holder of the successor, or its bearer for a belief of
    /// order 0; the cause's `observer` is someone else.
    SupersessionCauseNotObserved {
        belief: BeliefId,
        cause: EventId,
        observer: String,
        revising_mind: String,
    },
    /// The successor does not cite its cause: the cause is not among its
    /// `source_events`.
    SupersessionCauseNotCited {
        belief: BeliefId,
        cause: EventId,
    },
    /// The cause is not new: the belief replaced already rests on it.
    SupersessionCauseNotNew {
        belief: BeliefId,
        cause: EventId,
        superseded: BeliefId,
    },
    /// The cause is older than the belief it replaces: its tick is before
    /// the `valid_from` of that belief.
    SupersessionCauseBeforeSuperseded {
        belief: BeliefId,
        cause: EventId,
        cause_tick: u64,
        superseded: BeliefId,
        superseded_from: u64,
    },
    /// The cause is later than the belief it is said to have brought about:
    /// its tick is after the successor's `valid_from`. With
    /// `supersession-cause-not-earlier` this gives
    /// `replaced.valid_from <= cause.tick <= successor.valid_from`, so a
    /// successor never starts before the belief it replaces (ADR-0030; this
    /// was the rule `supersession-monotone` of ADR-0029). With
    /// `belief-key-chained` it makes at most one belief of a key hold at any
    /// tick (`verification/proofs/{coq,agda}/KeyChain`).
    SupersessionCauseAfterSuccessor {
        belief: BeliefId,
        cause: EventId,
        cause_tick: u64,
        valid_from: u64,
    },
    EmptyNameInAscriptionPath(BeliefId),
    DigestMismatch {
        declared: String,
        computed: String,
    },
    /// The producer's status for a belief is not the status the caller's
    /// profile policy derives from that belief's interval (ADR-0016). The
    /// policy comes from the caller, never from the package, so a producer
    /// cannot certify its own beliefs.
    StatusDisagreesWithInterval {
        belief: BeliefId,
        declared: BeliefStatus,
        derived: BeliefStatus,
    },
    /// The package has no canonical bytes, so it has no digest. A typed
    /// package can fail only one way: an integer above `2^53 − 1`
    /// (`IntegerOutOfRange`, owner ruling Q1).
    NotCanonical(CanonicalFault),
}

impl PackageFault {
    /// The stable field path the fault is about (CAC-KERNEL §6).
    #[must_use]
    pub fn path(&self) -> String {
        match self {
            PackageFault::IncompatibleKernel { .. } => "manifest.kernel_version".into(),
            PackageFault::DigestMismatch { .. } => "manifest.digest".into(),
            PackageFault::DuplicateEventId(id) => format!("events[event_id={}]", id.0),
            PackageFault::NonMonotonicOrder { at, .. } => format!("events[event_id={}]", at.0),
            PackageFault::DuplicateBeliefId(id)
            | PackageFault::SupersessionCycle(id)
            | PackageFault::MissingProvenance(id)
            | PackageFault::EmptyNameInAscriptionPath(id) => {
                format!("beliefs[belief_id={}]", id.0)
            }
            PackageFault::NestingBeyondDeclaredMax { belief, .. } => {
                format!("beliefs[belief_id={}].holders", belief.0)
            }
            PackageFault::UnknownSourceEvent { belief, .. } => {
                format!("beliefs[belief_id={}].source_events", belief.0)
            }
            PackageFault::SupersedesUnknownBelief { belief, .. }
            | PackageFault::BeliefKeyNotChained { belief, .. } => {
                format!("beliefs[belief_id={}].supersedes", belief.0)
            }
            PackageFault::SupersessionCauseUnknown { belief, .. }
            | PackageFault::SupersessionCauseNotObserved { belief, .. }
            | PackageFault::SupersessionCauseNotCited { belief, .. }
            | PackageFault::SupersessionCauseNotNew { belief, .. }
            | PackageFault::SupersessionCauseBeforeSuperseded { belief, .. }
            | PackageFault::SupersessionCauseAfterSuccessor { belief, .. } => {
                format!("beliefs[belief_id={}].supersedes.because", belief.0)
            }
            PackageFault::StatusDisagreesWithInterval { belief, .. } => {
                format!("beliefs[belief_id={}].status", belief.0)
            }
            PackageFault::NotCanonical(fault) => fault.path().to_string(),
        }
    }

    /// Every rule name [`PackageFault::rule`] can return, in declaration
    /// order. Pinned to the variants by an exhaustive match in this crate's
    /// tests, and to planted packages by `tests/package_fault_coverage.rs`.
    pub const RULES: &'static [&'static str] = &[
        "kernel-version-compatible",
        "event-id-unique",
        "belief-id-unique",
        "events-monotonic",
        "nesting-bounded",
        "belief-has-provenance",
        "source-events-present",
        "supersedes-present",
        "supersession-acyclic",
        "belief-key-chained",
        "supersession-cause-present",
        "supersession-cause-observed",
        "supersession-cause-cited",
        "supersession-cause-new",
        "supersession-cause-not-earlier",
        "supersession-cause-not-later",
        "ascription-names-nonempty",
        "digest-matches",
        "status-agrees-with-interval",
        "IntegerOutOfRange",
    ];

    /// The contract rule that failed, by stable name.
    #[must_use]
    pub fn rule(&self) -> &'static str {
        match self {
            PackageFault::IncompatibleKernel { .. } => "kernel-version-compatible",
            PackageFault::DuplicateEventId(_) => "event-id-unique",
            PackageFault::DuplicateBeliefId(_) => "belief-id-unique",
            PackageFault::NonMonotonicOrder { .. } => "events-monotonic",
            PackageFault::NestingBeyondDeclaredMax { .. } => "nesting-bounded",
            PackageFault::MissingProvenance(_) => "belief-has-provenance",
            PackageFault::UnknownSourceEvent { .. } => "source-events-present",
            PackageFault::SupersedesUnknownBelief { .. } => "supersedes-present",
            PackageFault::SupersessionCycle(_) => "supersession-acyclic",
            PackageFault::BeliefKeyNotChained { .. } => "belief-key-chained",
            PackageFault::SupersessionCauseUnknown { .. } => "supersession-cause-present",
            PackageFault::SupersessionCauseNotObserved { .. } => "supersession-cause-observed",
            PackageFault::SupersessionCauseNotCited { .. } => "supersession-cause-cited",
            PackageFault::SupersessionCauseNotNew { .. } => "supersession-cause-new",
            PackageFault::SupersessionCauseBeforeSuperseded { .. } => {
                "supersession-cause-not-earlier"
            }
            PackageFault::SupersessionCauseAfterSuccessor { .. } => "supersession-cause-not-later",
            PackageFault::EmptyNameInAscriptionPath(_) => "ascription-names-nonempty",
            PackageFault::DigestMismatch { .. } => "digest-matches",
            PackageFault::StatusDisagreesWithInterval { .. } => "status-agrees-with-interval",
            PackageFault::NotCanonical(fault) => fault.rule(),
        }
    }
}

/// The content digest of a package: lowercase hex SHA-256 over the RFC 8785
/// canonical bytes of the package with `manifest.digest` omitted.
///
/// The bytes are those of the package's typed form, so an absent optional
/// member is omitted rather than written as `null`, and an empty `holders`
/// is omitted. A producer in another language must hash that same form; the
/// conformance vectors pin it.
///
/// # Errors
///
/// `IntegerOutOfRange` if any integer exceeds `2^53 − 1`.
pub fn package_digest(package: &EpistemicPackage) -> Result<String, CanonicalFault> {
    let mut value = canonical::to_value(package)?;
    if let Value::Object(members) = &mut value
        && let Some((_, Value::Object(manifest))) =
            members.iter_mut().find(|(k, _)| k == "manifest")
    {
        manifest.retain(|(k, _)| k != "digest");
    }
    Ok(canonical::sha256_hex(&canonical::to_jcs(&value)))
}

/// Validate a package against a host, collecting **every** fault rather than
/// stopping at the first — the same contract as `Trace::validate`, because a
/// producer fixing one fault at a time from single-fault reports is a
/// miserable loop.
///
/// The digest is computed here, by [`package_digest`], and compared with
/// `manifest.digest`. No call skips it and no caller supplies it.
///
/// `status` is the caller's profile policy (ADR-0016). Every belief's
/// declared status must equal the status that policy derives from the
/// belief's interval. The policy is a required argument, resolved by the
/// caller from the profile, never read from the package: a package that
/// carried its own policy could certify itself.
///
/// # Errors
///
/// Every fault found, in a stable order.
pub fn validate_package(
    package: &EpistemicPackage,
    host: ContractVersion,
    depth: &DepthPolicy,
    status: &StatusPolicy,
) -> Result<(), Vec<PackageFault>> {
    let mut faults = Vec::new();

    // Manifest.
    if !host.accepts(package.manifest.kernel_version) {
        faults.push(PackageFault::IncompatibleKernel {
            declared: package.manifest.kernel_version,
            host,
        });
    }
    match package_digest(package) {
        Ok(computed) if computed == package.manifest.digest => {}
        Ok(computed) => faults.push(PackageFault::DigestMismatch {
            declared: package.manifest.digest.clone(),
            computed,
        }),
        Err(fault) => faults.push(PackageFault::NotCanonical(fault)),
    }

    // Events: unique ids, strictly increasing (tick, sequence). Equal stamps
    // are non-monotonic too — a total order with ties is not a total order.
    let mut event_ids = BTreeSet::new();
    for event in &package.events {
        if !event_ids.insert(event.event_id) {
            faults.push(PackageFault::DuplicateEventId(event.event_id));
        }
    }
    for pair in package.events.windows(2) {
        if pair[1].stamp() <= pair[0].stamp() {
            faults.push(PackageFault::NonMonotonicOrder {
                at: pair[1].event_id,
                prev: pair[0].stamp(),
                this: pair[1].stamp(),
            });
        }
    }

    // Beliefs.
    let mut belief_ids = BTreeSet::new();
    let mut supersedes = BTreeMap::new();
    for belief in &package.beliefs {
        if !belief_ids.insert(belief.belief_id) {
            faults.push(PackageFault::DuplicateBeliefId(belief.belief_id));
        }
        if let Some(link) = belief.supersedes {
            supersedes.insert(belief.belief_id, link.belief);
        }
        if belief.order() > depth.max_order {
            faults.push(PackageFault::NestingBeyondDeclaredMax {
                belief: belief.belief_id,
                order: belief.order(),
                max: depth.max_order,
            });
        }
        if belief.holders.iter().any(String::is_empty) {
            faults.push(PackageFault::EmptyNameInAscriptionPath(belief.belief_id));
        }
        if belief.source_events.is_empty() {
            faults.push(PackageFault::MissingProvenance(belief.belief_id));
        }
        let derived = status.status(belief.confidence);
        if derived != belief.status {
            faults.push(PackageFault::StatusDisagreesWithInterval {
                belief: belief.belief_id,
                declared: belief.status,
                derived,
            });
        }
        for source in &belief.source_events {
            if !event_ids.contains(source) {
                faults.push(PackageFault::UnknownSourceEvent {
                    belief: belief.belief_id,
                    event: *source,
                });
            }
        }
    }
    // The first belief and the first event of each id; a repeated id is
    // `belief-id-unique`'s or `event-id-unique`'s to report.
    let mut beliefs_by_id: BTreeMap<BeliefId, &Belief> = BTreeMap::new();
    for belief in &package.beliefs {
        beliefs_by_id.entry(belief.belief_id).or_insert(belief);
    }
    // The events each belief cites, as a set, built once: a predecessor named
    // by many successors is then not rescanned for each of them, so the cause
    // rules stay near-linear in the size of the package.
    let cited_by_id: BTreeMap<BeliefId, BTreeSet<EventId>> = beliefs_by_id
        .iter()
        .map(|(id, belief)| (*id, belief.source_events.iter().copied().collect()))
        .collect();
    let mut events_by_id: BTreeMap<EventId, &EpistemicEvent> = BTreeMap::new();
    for event in &package.events {
        events_by_id.entry(event.event_id).or_insert(event);
    }
    for belief in &package.beliefs {
        if let Some(link) = belief.supersedes {
            let replaced = beliefs_by_id.get(&link.belief).copied();
            let replaced_cites = cited_by_id.get(&link.belief);
            if replaced.is_none() {
                faults.push(PackageFault::SupersedesUnknownBelief {
                    belief: belief.belief_id,
                    superseded: link.belief,
                });
            }
            cause_faults(
                belief,
                link,
                replaced.zip(replaced_cites),
                &events_by_id,
                &mut faults,
            );
        }
    }
    // Supersession cycles: `supersedes` is a functional graph (out-degree at
    // most one per belief), so this is a walk over chains, not a general
    // graph search. A three-colour marking — unvisited / in-progress (on the
    // walk currently underway) / done — visits every belief at most once in
    // total across *all* starts, so a hostile linear chain of n beliefs
    // costs O(n) rather than the O(n^2) a fresh per-start `seen` set would
    // give (a package loader is exactly where that assumption must not be
    // made). It also lets a genuine cycle be reported exactly once, naming
    // a belief that is actually on it — not whichever start happened to
    // walk into it first.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        InProgress,
        Done,
    }
    let mut marks: BTreeMap<BeliefId, Mark> = BTreeMap::new();
    for &start in supersedes.keys() {
        if marks.contains_key(&start) {
            continue;
        }
        let mut path = Vec::new();
        let mut current = start;
        loop {
            match marks.get(&current) {
                Some(Mark::Done) => break, // joins an already-resolved chain
                Some(Mark::InProgress) => {
                    // `current` closes a cycle back onto this walk's path.
                    // Report the smallest id actually on the cycle, once.
                    let cycle_start = path
                        .iter()
                        .position(|&id| id == current)
                        .expect("an in-progress belief is always on this walk's path");
                    let cycle_member = path[cycle_start..].iter().copied().min().unwrap_or(current);
                    faults.push(PackageFault::SupersessionCycle(cycle_member));
                    break;
                }
                None => {
                    marks.insert(current, Mark::InProgress);
                    path.push(current);
                    match supersedes.get(&current) {
                        Some(&next) => current = next,
                        None => break, // chain ends cleanly, no cycle
                    }
                }
            }
        }
        for id in path {
            marks.insert(id, Mark::Done);
        }
    }

    key_chain_faults(&package.beliefs, &mut faults);

    if faults.is_empty() {
        Ok(())
    } else {
        Err(faults)
    }
}

/// The six rules on the cause of one supersession link (ADR-0030).
///
/// `replaced` is the belief the link names with the set of events it cites,
/// if the package has it, and `events` the package's events by id. Reported in
/// this order: `supersession-cause-present`, `-observed`, `-cited`, `-new`,
/// `-not-earlier`, `-not-later`.
///
/// A rule that needs a thing the package lacks is not evaluated, and the
/// lack is some other rule's fault. With no event for `because` the first
/// rule is the only one reported. With no belief to replace
/// (`supersedes-present`), the two rules that read the replaced belief
/// (`-new` and `-not-earlier`) are skipped and the other four still run.
fn cause_faults(
    belief: &Belief,
    link: Supersession,
    replaced: Option<(&Belief, &BTreeSet<EventId>)>,
    events: &BTreeMap<EventId, &EpistemicEvent>,
    faults: &mut Vec<PackageFault>,
) {
    let Some(cause) = events.get(&link.because).copied() else {
        faults.push(PackageFault::SupersessionCauseUnknown {
            belief: belief.belief_id,
            cause: link.because,
        });
        return;
    };
    let revising_mind = revising_mind(belief);
    if cause.observer != revising_mind {
        faults.push(PackageFault::SupersessionCauseNotObserved {
            belief: belief.belief_id,
            cause: link.because,
            observer: cause.observer.clone(),
            revising_mind: revising_mind.to_string(),
        });
    }
    if !belief.source_events.contains(&link.because) {
        faults.push(PackageFault::SupersessionCauseNotCited {
            belief: belief.belief_id,
            cause: link.because,
        });
    }
    if let Some((replaced, replaced_cites)) = replaced {
        if replaced_cites.contains(&link.because) {
            faults.push(PackageFault::SupersessionCauseNotNew {
                belief: belief.belief_id,
                cause: link.because,
                superseded: link.belief,
            });
        }
        if cause.tick < replaced.valid_from {
            faults.push(PackageFault::SupersessionCauseBeforeSuperseded {
                belief: belief.belief_id,
                cause: link.because,
                cause_tick: cause.tick,
                superseded: link.belief,
                superseded_from: replaced.valid_from,
            });
        }
    }
    if cause.tick > belief.valid_from {
        faults.push(PackageFault::SupersessionCauseAfterSuccessor {
            belief: belief.belief_id,
            cause: link.because,
            cause_tick: cause.tick,
            valid_from: belief.valid_from,
        });
    }
}

/// The mind in which a belief lives, and so the mind that revises it: the
/// outermost holder of the ascription path, or the bearer for a belief of
/// order 0. `holders = ["guard-0"]`, `bearer = "contact-21"` is guard-0's
/// model, revised on what guard-0 receives.
fn revising_mind(belief: &Belief) -> &str {
    belief.holders.first().unwrap_or(&belief.bearer)
}

/// CAC-KERNEL §3 invariant 4, made checkable (ADR-0029): the beliefs that
/// share one key (`holders`, `bearer`, `proposition`) form one chain under
/// `supersedes`.
///
/// Only edges that stay inside the key count, so a supersession across keys
/// is neither required nor refused here. For a key with `n` beliefs, an
/// acyclic set of in-key edges with out-degree at most 1 (a belief
/// supersedes at most one) and in-degree at most 1 (no fork) is a union of
/// disjoint paths, and it is one path exactly when it has `n − 1` edges.
/// Cycles are `supersession-acyclic`'s to report; this rule is checked on
/// in-degree and on the number of chain starts, which is cheap and needs no
/// graph search.
///
/// Reported, in a stable order:
/// * each fork: every belief that supersedes an already-superseded belief
///   of its key, after the first by id (`superseded: Some(t)`);
/// * each silent overwrite: every belief of the key that supersedes no
///   belief of the key, other than the earliest by (`valid_from`, id)
///   (`superseded: None`).
fn key_chain_faults(beliefs: &[Belief], faults: &mut Vec<PackageFault>) {
    type Key<'a> = (&'a [String], &'a str, &'a str);
    let mut keys: BTreeMap<Key<'_>, Vec<&Belief>> = BTreeMap::new();
    for belief in beliefs {
        keys.entry((&belief.holders, &belief.bearer, &belief.proposition.0))
            .or_default()
            .push(belief);
    }
    for members in keys.values() {
        if members.len() < 2 {
            continue;
        }
        let ids: BTreeSet<BeliefId> = members.iter().map(|b| b.belief_id).collect();
        // Forks: one superseded belief claimed twice inside the key.
        let mut superseders: BTreeMap<BeliefId, Vec<BeliefId>> = BTreeMap::new();
        for belief in members {
            if let Some(link) = belief.supersedes
                && ids.contains(&link.belief)
            {
                superseders
                    .entry(link.belief)
                    .or_default()
                    .push(belief.belief_id);
            }
        }
        for (target, mut by) in superseders {
            by.sort_unstable();
            for &extra in by.iter().skip(1) {
                faults.push(PackageFault::BeliefKeyNotChained {
                    belief: extra,
                    superseded: Some(target),
                });
            }
        }
        // Silent overwrites: more than one chain start in the key.
        let mut starts: Vec<&Belief> = members
            .iter()
            .copied()
            .filter(|b| !b.supersedes.is_some_and(|link| ids.contains(&link.belief)))
            .collect();
        starts.sort_unstable_by_key(|b| (b.valid_from, b.belief_id));
        for start in starts.iter().skip(1) {
            faults.push(PackageFault::BeliefKeyNotChained {
                belief: start.belief_id,
                superseded: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::belief::{
        BeliefInterval, BeliefStatus, EventKind, Proposition, Provenance, Subject, Supersession,
    };
    use crate::version::KERNEL_CONTRACT_VERSION;
    use enaction_trace::Mass;

    fn manifest() -> PackageManifest {
        PackageManifest {
            kernel_version: KERNEL_CONTRACT_VERSION,
            profile: ProfileRef {
                id: "idaptik/esm/v1".into(),
                version: ContractVersion { major: 1, minor: 0 },
                targets_kernel: KERNEL_CONTRACT_VERSION,
            },
            schema_version: ContractVersion { major: 1, minor: 0 },
            source: "ums:ghost-lobby".into(),
            digest: "abc123".into(),
            migration_from: ContractVersion { major: 1, minor: 0 },
            seed: SeedPolicy::Fixed(7),
            guarantees: vec!["esm/replay-deterministic".into()],
        }
    }

    fn event(id: u64, tick: u64, sequence: u64) -> EpistemicEvent {
        EpistemicEvent {
            event_id: EventId(id),
            sequence,
            tick,
            observer: "guard".into(),
            subject: Subject::Agent("intruder".into()),
            kind: EventKind::Observation,
            proposition: Proposition("intruder in lobby".into()),
            confidence: Mass::new(8_000).unwrap(),
            provenance: Provenance {
                origin: "sight".into(),
                channel: Some("visual".into()),
            },
            affect: None,
            conation: None,
        }
    }

    fn belief(id: u64, holders: &[&str], sources: &[u64]) -> Belief {
        Belief {
            belief_id: BeliefId(id),
            holders: holders.iter().map(|h| (*h).to_string()).collect(),
            bearer: "guard".into(),
            proposition: Proposition("intruder in lobby".into()),
            status: BeliefStatus::Believed,
            confidence: BeliefInterval::new(Mass::new(8_000).unwrap(), Mass::new(9_500).unwrap())
                .unwrap(),
            source_events: sources.iter().map(|s| EventId(*s)).collect(),
            valid_from: 1,
            supersedes: None,
        }
    }

    /// A supersession link: replaces `belief` because of event `because`.
    fn link(belief: u64, because: u64) -> Supersession {
        Supersession {
            belief: BeliefId(belief),
            because: EventId(because),
        }
    }

    /// A small valid Ghost-Lobby-shaped package: two events, the guard's own
    /// belief, and one order-1 ascription of it.
    fn ghost_lobby() -> EpistemicPackage {
        sealed(EpistemicPackage {
            manifest: manifest(),
            events: vec![event(1, 1, 1), event(2, 1, 2)],
            beliefs: vec![belief(1, &[], &[1]), belief(2, &["observer"], &[2])],
        })
    }

    fn policy() -> StatusPolicy {
        StatusPolicy {
            id: "test/status/v1".into(),
            rejected_ceiling: Mass::new(1_000).unwrap(),
            unknown_band: Mass::new(6_000).unwrap(),
            believed_floor: Mass::new(7_000).unwrap(),
            suspected_floor: Mass::new(5_000).unwrap(),
        }
    }

    /// Validate after resealing, so each test sees only the fault it
    /// planted. The digest test below calls `validate_package` directly.
    fn validate(package: &EpistemicPackage) -> Result<(), Vec<PackageFault>> {
        validate_package(
            &sealed(package.clone()),
            KERNEL_CONTRACT_VERSION,
            &DepthPolicy::default(),
            &policy(),
        )
    }

    fn sealed(mut package: EpistemicPackage) -> EpistemicPackage {
        package.manifest.digest = package_digest(&package).unwrap();
        package
    }

    #[test]
    fn rules_lists_every_variant_exactly_once() {
        // Exhaustive on purpose: a new variant fails to compile here until it
        // is given a representative, and then fails the assertion until its
        // rule is in RULES.
        fn representative(f: &PackageFault) -> PackageFault {
            match f {
                PackageFault::IncompatibleKernel { .. }
                | PackageFault::DuplicateEventId(_)
                | PackageFault::DuplicateBeliefId(_)
                | PackageFault::NonMonotonicOrder { .. }
                | PackageFault::NestingBeyondDeclaredMax { .. }
                | PackageFault::MissingProvenance(_)
                | PackageFault::UnknownSourceEvent { .. }
                | PackageFault::SupersedesUnknownBelief { .. }
                | PackageFault::SupersessionCycle(_)
                | PackageFault::BeliefKeyNotChained { .. }
                | PackageFault::SupersessionCauseUnknown { .. }
                | PackageFault::SupersessionCauseNotObserved { .. }
                | PackageFault::SupersessionCauseNotCited { .. }
                | PackageFault::SupersessionCauseNotNew { .. }
                | PackageFault::SupersessionCauseBeforeSuperseded { .. }
                | PackageFault::SupersessionCauseAfterSuccessor { .. }
                | PackageFault::EmptyNameInAscriptionPath(_)
                | PackageFault::DigestMismatch { .. }
                | PackageFault::StatusDisagreesWithInterval { .. }
                | PackageFault::NotCanonical(_) => f.clone(),
            }
        }
        let b = BeliefId(1);
        let e = EventId(1);
        let all = [
            PackageFault::IncompatibleKernel {
                declared: KERNEL_CONTRACT_VERSION,
                host: KERNEL_CONTRACT_VERSION,
            },
            PackageFault::DuplicateEventId(e),
            PackageFault::DuplicateBeliefId(b),
            PackageFault::NonMonotonicOrder {
                at: e,
                prev: (0, 0),
                this: (0, 0),
            },
            PackageFault::NestingBeyondDeclaredMax {
                belief: b,
                order: 0,
                max: 0,
            },
            PackageFault::MissingProvenance(b),
            PackageFault::UnknownSourceEvent {
                belief: b,
                event: e,
            },
            PackageFault::SupersedesUnknownBelief {
                belief: b,
                superseded: b,
            },
            PackageFault::SupersessionCycle(b),
            PackageFault::BeliefKeyNotChained {
                belief: b,
                superseded: None,
            },
            PackageFault::SupersessionCauseUnknown {
                belief: b,
                cause: e,
            },
            PackageFault::SupersessionCauseNotObserved {
                belief: b,
                cause: e,
                observer: String::new(),
                revising_mind: String::new(),
            },
            PackageFault::SupersessionCauseNotCited {
                belief: b,
                cause: e,
            },
            PackageFault::SupersessionCauseNotNew {
                belief: b,
                cause: e,
                superseded: b,
            },
            PackageFault::SupersessionCauseBeforeSuperseded {
                belief: b,
                cause: e,
                cause_tick: 0,
                superseded: b,
                superseded_from: 0,
            },
            PackageFault::SupersessionCauseAfterSuccessor {
                belief: b,
                cause: e,
                cause_tick: 0,
                valid_from: 0,
            },
            PackageFault::EmptyNameInAscriptionPath(b),
            PackageFault::DigestMismatch {
                declared: String::new(),
                computed: String::new(),
            },
            PackageFault::StatusDisagreesWithInterval {
                belief: b,
                declared: BeliefStatus::Certain,
                derived: BeliefStatus::Certain,
            },
            PackageFault::NotCanonical(canonical::to_value(&u64::MAX).expect_err("out of range")),
        ];
        let rules: Vec<_> = all.iter().map(|f| representative(f).rule()).collect();
        assert_eq!(rules, PackageFault::RULES);
    }

    #[test]
    fn a_valid_package_validates() {
        assert_eq!(validate(&ghost_lobby()), Ok(()));
    }

    // ------------------------------------------------ belief-key-chained

    /// The guard's own belief about the lobby, held from `valid_from`. It
    /// rests on event `id` and, when it supersedes, names that same event as
    /// its cause: new, cited, and received by the guard. [`chain`] puts
    /// event `i` on tick `i`, so a link is in order when
    /// `replaced.valid_from <= id <= valid_from`.
    fn own(id: u64, valid_from: u64, supersedes: Option<u64>) -> Belief {
        Belief {
            valid_from,
            supersedes: supersedes.map(|target| link(target, id)),
            ..belief(id, &[], &[id])
        }
    }

    /// The ghost lobby with events 1..=9 (event `i` on tick `i`, the guard's)
    /// and these beliefs.
    fn chain(beliefs: Vec<Belief>) -> EpistemicPackage {
        EpistemicPackage {
            events: (1..=9).map(|i| event(i, i, 1)).collect(),
            beliefs,
            ..ghost_lobby()
        }
    }

    fn key_faults(beliefs: Vec<Belief>) -> Vec<PackageFault> {
        match validate(&chain(beliefs)) {
            Ok(()) => Vec::new(),
            Err(faults) => faults
                .into_iter()
                .filter(|f| f.rule() == "belief-key-chained")
                .collect(),
        }
    }

    #[test]
    fn a_linked_chain_of_one_key_is_accepted() {
        assert_eq!(
            validate(&chain(vec![
                own(1, 1, None),
                own(2, 3, Some(1)),
                own(3, 5, Some(2))
            ])),
            Ok(())
        );
    }

    #[test]
    fn an_unlinked_second_belief_of_one_key_is_a_silent_overwrite() {
        // The falsifier of ENACTIVE-COMMITMENTS C2 (a), as a package.
        assert_eq!(
            key_faults(vec![own(1, 1, None), own(2, 3, None)]),
            [PackageFault::BeliefKeyNotChained {
                belief: BeliefId(2),
                superseded: None
            }]
        );
    }

    #[test]
    fn an_unlinked_restatement_is_refused_even_when_it_agrees() {
        // Same status, same interval, no link: still two heads for one key.
        let faults = key_faults(vec![own(1, 1, None), own(2, 1, None)]);
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert_eq!(faults[0].path(), "beliefs[belief_id=2].supersedes");
    }

    #[test]
    fn the_earliest_start_is_kept_whatever_the_listing_order() {
        assert_eq!(
            key_faults(vec![own(7, 9, None), own(3, 2, None)]),
            [PackageFault::BeliefKeyNotChained {
                belief: BeliefId(7),
                superseded: None
            }]
        );
    }

    #[test]
    fn two_successors_of_one_belief_are_a_fork() {
        assert_eq!(
            key_faults(vec![
                own(1, 1, None),
                own(2, 3, Some(1)),
                own(3, 3, Some(1))
            ]),
            [PackageFault::BeliefKeyNotChained {
                belief: BeliefId(3),
                superseded: Some(BeliefId(1))
            }]
        );
    }

    #[test]
    fn different_keys_never_need_a_link() {
        // Same bearer and proposition, different ascription path; and the
        // same path about a different proposition.
        let mut other_prop = own(3, 1, None);
        other_prop.proposition = Proposition("intruder in hall".into());
        let mut other_bearer = own(4, 1, None);
        other_bearer.bearer = "observer".into();
        assert_eq!(
            key_faults(vec![
                own(1, 1, None),
                belief(2, &["observer"], &[1]),
                other_prop,
                other_bearer,
            ]),
            []
        );
    }

    #[test]
    fn a_supersession_across_keys_does_not_link_a_key() {
        // 2 supersedes a belief of another key, so inside its own key it is
        // a second start: the cross-key edge does not excuse the overwrite.
        let mut elsewhere = own(9, 1, None);
        elsewhere.proposition = Proposition("intruder in hall".into());
        assert_eq!(
            key_faults(vec![own(1, 1, None), elsewhere, own(2, 3, Some(9))]),
            [PackageFault::BeliefKeyNotChained {
                belief: BeliefId(2),
                superseded: None
            }]
        );
    }

    // ---------------------------------------- the link names its cause

    /// The guard's events: 1 on tick 1, 2 and 4 on tick 3 (4 is the
    /// intruder's), 3 on tick 5.
    fn cause_events() -> Vec<EpistemicEvent> {
        let mut intruder = event(4, 3, 2);
        intruder.observer = "intruder".into();
        vec![event(1, 1, 1), event(2, 3, 1), intruder, event(3, 5, 1)]
    }

    /// Beliefs 1 and 2 of the guard's own key, each as (`valid_from`,
    /// sources): 2 replaces 1 because of event `because`.
    fn pair(old: (u64, &[u64]), new: (u64, &[u64]), because: u64) -> Vec<Belief> {
        vec![
            Belief {
                valid_from: old.0,
                ..belief(1, &[], old.1)
            },
            Belief {
                valid_from: new.0,
                supersedes: Some(link(1, because)),
                ..belief(2, &[], new.1)
            },
        ]
    }

    /// Only the faults of the six cause rules.
    fn cause_faults(beliefs: Vec<Belief>) -> Vec<PackageFault> {
        let package = EpistemicPackage {
            events: cause_events(),
            beliefs,
            ..ghost_lobby()
        };
        match validate(&package) {
            Ok(()) => Vec::new(),
            Err(faults) => faults
                .into_iter()
                .filter(|f| f.rule().starts_with("supersession-cause-"))
                .collect(),
        }
    }

    fn rules(faults: &[PackageFault]) -> Vec<&'static str> {
        faults.iter().map(PackageFault::rule).collect()
    }

    #[test]
    fn a_link_in_order_with_new_cited_received_evidence_is_accepted() {
        // Event 2 is on tick 3, between 2 and 4, and nothing else.
        assert_eq!(cause_faults(pair((2, &[1]), (4, &[2]), 2)), []);
    }

    #[test]
    fn a_cause_not_in_the_package_is_refused_and_ends_the_cause_rules() {
        let faults = cause_faults(pair((2, &[1]), (4, &[2]), 99));
        assert_eq!(
            faults,
            [PackageFault::SupersessionCauseUnknown {
                belief: BeliefId(2),
                cause: EventId(99)
            }]
        );
        assert_eq!(faults[0].rule(), "supersession-cause-present");
        assert_eq!(faults[0].path(), "beliefs[belief_id=2].supersedes.because");
    }

    #[test]
    fn a_cause_the_revising_mind_did_not_receive_is_refused() {
        // Event 4 reaches the intruder; the guard's own belief is not
        // revised because of it.
        let faults = cause_faults(pair((2, &[1]), (4, &[4]), 4));
        assert_eq!(
            faults,
            [PackageFault::SupersessionCauseNotObserved {
                belief: BeliefId(2),
                cause: EventId(4),
                observer: "intruder".into(),
                revising_mind: "guard".into()
            }]
        );
        assert_eq!(faults[0].rule(), "supersession-cause-observed");
        assert_eq!(faults[0].path(), "beliefs[belief_id=2].supersedes.because");
    }

    #[test]
    fn a_model_is_revised_in_its_outermost_holders_mind() {
        // The intruder's model of the guard (holders ["intruder"], bearer
        // "guard") lives in the intruder's mind: it is revised on what the
        // intruder receives (event 4), not on what the guard does (event 2).
        let model = |id: u64, from: u64, src: u64, because: Option<u64>| Belief {
            valid_from: from,
            supersedes: because.map(|e| link(id - 1, e)),
            ..belief(id, &["intruder"], &[src])
        };
        assert_eq!(
            cause_faults(vec![model(1, 2, 1, None), model(2, 4, 4, Some(4))]),
            []
        );
        let faults = cause_faults(vec![model(1, 2, 1, None), model(2, 4, 2, Some(2))]);
        assert_eq!(
            rules(&faults),
            ["supersession-cause-observed"],
            "{faults:?}"
        );
    }

    #[test]
    fn a_cause_the_successor_does_not_cite_is_refused() {
        let faults = cause_faults(pair((2, &[1]), (4, &[3]), 2));
        assert_eq!(
            faults,
            [PackageFault::SupersessionCauseNotCited {
                belief: BeliefId(2),
                cause: EventId(2)
            }]
        );
        assert_eq!(faults[0].rule(), "supersession-cause-cited");
    }

    #[test]
    fn a_cause_the_replaced_belief_already_rests_on_is_not_new() {
        let faults = cause_faults(pair((2, &[1, 2]), (4, &[2]), 2));
        assert_eq!(
            faults,
            [PackageFault::SupersessionCauseNotNew {
                belief: BeliefId(2),
                cause: EventId(2),
                superseded: BeliefId(1)
            }]
        );
        assert_eq!(faults[0].rule(), "supersession-cause-new");
    }

    #[test]
    fn a_cause_before_the_replaced_belief_started_is_refused() {
        // Event 2 is on tick 3; belief 1 starts on tick 4.
        let faults = cause_faults(pair((4, &[1]), (6, &[2]), 2));
        assert_eq!(
            faults,
            [PackageFault::SupersessionCauseBeforeSuperseded {
                belief: BeliefId(2),
                cause: EventId(2),
                cause_tick: 3,
                superseded: BeliefId(1),
                superseded_from: 4
            }]
        );
        assert_eq!(faults[0].rule(), "supersession-cause-not-earlier");
    }

    #[test]
    fn a_cause_after_the_successor_started_is_refused() {
        // Event 2 is on tick 3; belief 2 starts on tick 2.
        let faults = cause_faults(pair((1, &[1]), (2, &[2]), 2));
        assert_eq!(
            faults,
            [PackageFault::SupersessionCauseAfterSuccessor {
                belief: BeliefId(2),
                cause: EventId(2),
                cause_tick: 3,
                valid_from: 2
            }]
        );
        assert_eq!(faults[0].rule(), "supersession-cause-not-later");
    }

    #[test]
    fn the_cause_may_sit_on_either_boundary_tick() {
        // Event 2 is on tick 3: the replaced belief starts there, and so
        // does the successor.
        assert_eq!(cause_faults(pair((3, &[1]), (3, &[2]), 2)), []);
    }

    #[test]
    fn a_successor_that_starts_before_what_it_replaces_fails_a_cause_rule_whatever_the_cause() {
        // ADR-0030: monotonicity is not a rule. It follows from the last two
        // cause rules, so for every cause tick one of them fires.
        for tick in 0..=10 {
            let package = EpistemicPackage {
                events: vec![event(1, tick, 1)],
                beliefs: vec![
                    Belief {
                        valid_from: 6,
                        ..belief(1, &[], &[1])
                    },
                    Belief {
                        valid_from: 5,
                        supersedes: Some(link(1, 1)),
                        ..belief(2, &[], &[1])
                    },
                ],
                ..ghost_lobby()
            };
            let faults = validate(&package).unwrap_err();
            let timing = faults.iter().any(|f| {
                matches!(
                    f.rule(),
                    "supersession-cause-not-earlier" | "supersession-cause-not-later"
                )
            });
            assert!(timing, "cause on tick {tick}: {faults:?}");
        }
    }

    #[test]
    fn every_cause_rule_that_fires_is_reported_in_rule_order() {
        // Cause 4 (the intruder's, tick 3) for a guard belief starting on
        // tick 2 that replaces a belief starting on tick 4 and resting on 4.
        let faults = cause_faults(pair((4, &[4]), (2, &[1]), 4));
        assert_eq!(
            rules(&faults),
            [
                "supersession-cause-observed",
                "supersession-cause-cited",
                "supersession-cause-new",
                "supersession-cause-not-earlier",
                "supersession-cause-not-later",
            ]
        );
    }

    #[test]
    fn a_link_to_an_unknown_belief_skips_only_the_rules_that_read_it() {
        // The replaced belief is missing: `supersedes-present` reports it.
        // `-new` and `-not-earlier` read that belief and are skipped; the
        // other four still run.
        let package = |because| EpistemicPackage {
            events: cause_events(),
            beliefs: vec![Belief {
                valid_from: 2,
                supersedes: Some(link(42, because)),
                ..belief(1, &[], &[1])
            }],
            ..ghost_lobby()
        };
        let faults = validate(&package(2)).unwrap_err();
        let reported = rules(&faults);
        assert!(reported.contains(&"supersedes-present"), "{reported:?}");
        assert!(
            reported.contains(&"supersession-cause-cited"),
            "cause 2 is not among the sources [1]: {reported:?}"
        );
        assert!(reported.contains(&"supersession-cause-not-later"));
        assert!(!reported.contains(&"supersession-cause-new"));
        assert!(!reported.contains(&"supersession-cause-not-earlier"));
        // With no cause either, only the first cause rule speaks.
        let faults = validate(&package(99)).unwrap_err();
        let cause: Vec<_> = rules(&faults)
            .into_iter()
            .filter(|r| r.starts_with("supersession-cause-"))
            .collect();
        assert_eq!(cause, ["supersession-cause-present"]);
    }

    #[test]
    fn a_belief_that_supersedes_itself_does_not_panic_the_cause_rules() {
        let package = EpistemicPackage {
            events: cause_events(),
            beliefs: vec![Belief {
                valid_from: 4,
                supersedes: Some(link(1, 2)),
                ..belief(1, &[], &[2])
            }],
            ..ghost_lobby()
        };
        let faults = validate(&package).unwrap_err();
        assert!(rules(&faults).contains(&"supersession-acyclic"));
        assert!(rules(&faults).contains(&"supersession-cause-new"));
    }

    #[test]
    fn a_wrong_kernel_major_is_rejected() {
        let mut package = ghost_lobby();
        // The host is 2.0 (ADR-0030): a 1.x package is the older major, which
        // the contract refuses by rule; the next major is one the host does
        // not know.
        for major in [1, KERNEL_CONTRACT_VERSION.major + 1] {
            package.manifest.kernel_version = ContractVersion { major, minor: 0 };
            let faults = validate(&package).unwrap_err();
            assert!(
                matches!(faults[0], PackageFault::IncompatibleKernel { .. }),
                "major {major}: {faults:?}"
            );
            assert_eq!(faults[0].rule(), "kernel-version-compatible");
        }
    }

    #[test]
    fn the_host_is_kernel_2_0_and_refuses_every_1_x() {
        assert_eq!(
            KERNEL_CONTRACT_VERSION,
            ContractVersion { major: 2, minor: 0 }
        );
        for minor in [0, 1, 9] {
            let mut package = ghost_lobby();
            package.manifest.kernel_version = ContractVersion { major: 1, minor };
            let faults = validate(&package).unwrap_err();
            assert_eq!(faults[0].rule(), "kernel-version-compatible", "1.{minor}");
        }
    }

    #[test]
    fn a_newer_minor_than_the_host_is_rejected() {
        let mut package = ghost_lobby();
        // Same major as the host, one minor beyond it: refused for the minor,
        // not for the major.
        package.manifest.kernel_version = ContractVersion {
            major: KERNEL_CONTRACT_VERSION.major,
            minor: KERNEL_CONTRACT_VERSION.minor + 1,
        };
        let faults = validate(&package).unwrap_err();
        assert_eq!(faults[0].rule(), "kernel-version-compatible");
    }

    #[test]
    fn duplicate_ids_are_rejected_for_events_and_beliefs() {
        let mut package = ghost_lobby();
        package.events.push(event(1, 2, 3)); // id 1 again
        package.beliefs.push(belief(1, &[], &[1])); // id 1 again
        let faults = validate(&package).unwrap_err();
        assert!(faults.contains(&PackageFault::DuplicateEventId(EventId(1))));
        assert!(faults.contains(&PackageFault::DuplicateBeliefId(BeliefId(1))));
    }

    #[test]
    fn decreasing_and_equal_stamps_are_both_non_monotonic() {
        let mut package = ghost_lobby();
        package.events = vec![event(1, 2, 1), event(2, 1, 9)]; // tick goes back
        let faults = validate(&package).unwrap_err();
        assert!(matches!(
            faults
                .iter()
                .find(|f| f.rule() == "events-monotonic")
                .unwrap(),
            PackageFault::NonMonotonicOrder { .. }
        ));

        let mut tied = ghost_lobby();
        tied.events = vec![event(1, 1, 1), event(2, 1, 1)]; // equal stamp
        assert!(
            validate(&tied)
                .unwrap_err()
                .iter()
                .any(|f| f.rule() == "events-monotonic"),
            "a total order with ties is not a total order"
        );
    }

    #[test]
    fn nesting_is_bounded_at_the_declared_maximum_exactly() {
        let mut package = ghost_lobby();
        package.beliefs.push(belief(3, &["a", "b"], &[1])); // order 2 > max 1
        let faults = validate(&package).unwrap_err();
        assert!(matches!(
            faults[0],
            PackageFault::NestingBeyondDeclaredMax {
                order: 2,
                max: 1,
                ..
            }
        ));
        // Exactly at the maximum is accepted — the bound is a bound, not an
        // off-by-one.
        assert_eq!(validate(&ghost_lobby()), Ok(()));
    }

    #[test]
    fn a_belief_without_provenance_is_rejected() {
        let mut package = ghost_lobby();
        package.beliefs.push(belief(3, &[], &[]));
        let faults = validate(&package).unwrap_err();
        assert!(faults.contains(&PackageFault::MissingProvenance(BeliefId(3))));
        assert_eq!(
            faults[0].path(),
            "beliefs[belief_id=3]",
            "the diagnostic names the field path"
        );
    }

    #[test]
    fn a_source_event_the_package_lacks_is_rejected() {
        let mut package = ghost_lobby();
        package.beliefs.push(belief(3, &[], &[99]));
        let faults = validate(&package).unwrap_err();
        assert!(faults.contains(&PackageFault::UnknownSourceEvent {
            belief: BeliefId(3),
            event: EventId(99),
        }));
    }

    #[test]
    fn supersession_must_name_a_real_belief_and_must_not_loop() {
        let mut package = ghost_lobby();
        package.beliefs[0].supersedes = Some(link(42, 1));
        let faults = validate(&package).unwrap_err();
        assert!(faults.contains(&PackageFault::SupersedesUnknownBelief {
            belief: BeliefId(1),
            superseded: BeliefId(42),
        }));

        let mut cyclic = ghost_lobby();
        cyclic.beliefs[0].supersedes = Some(link(2, 1));
        cyclic.beliefs[1].supersedes = Some(link(1, 1));
        let faults = validate(&cyclic).unwrap_err();
        assert!(
            faults
                .iter()
                .any(|f| matches!(f, PackageFault::SupersessionCycle(_))),
            "contradiction history is a line, not a circle: {faults:?}"
        );
    }

    #[test]
    fn an_empty_name_in_an_ascription_path_is_rejected() {
        let mut package = ghost_lobby();
        package.beliefs.push(belief(3, &[""], &[1]));
        let faults = validate(&package).unwrap_err();
        assert!(faults.contains(&PackageFault::EmptyNameInAscriptionPath(BeliefId(3))));
    }

    #[test]
    fn the_digest_is_always_compared() {
        let mut package = ghost_lobby();
        assert_eq!(validate(&package), Ok(()));
        package.manifest.digest = "something-else".into();
        let faults = validate_package(
            &package,
            KERNEL_CONTRACT_VERSION,
            &DepthPolicy::default(),
            &policy(),
        )
        .unwrap_err();
        assert!(matches!(faults[0], PackageFault::DigestMismatch { .. }));
        assert_eq!(faults[0].rule(), "digest-matches");
    }

    #[test]
    fn every_fault_is_reported_not_only_the_first() {
        let mut package = ghost_lobby();
        package.manifest.kernel_version = ContractVersion { major: 9, minor: 0 };
        package.events.push(event(1, 0, 0)); // duplicate id AND non-monotonic
        package.beliefs.push(belief(3, &["a", "b", ""], &[]));
        let faults = validate(&package).unwrap_err();
        let rules: BTreeSet<_> = faults.iter().map(PackageFault::rule).collect();
        assert!(rules.len() >= 5, "expected many distinct rules: {rules:?}");
    }

    #[test]
    fn a_supersession_cycle_names_a_belief_actually_on_the_cycle_exactly_once() {
        // 1 -> 2 -> 3 -> 2: belief 1 is only the lead-in, not part of the
        // cycle (which is 2 -> 3 -> 2). The diagnostic must name a belief
        // that IS on the cycle, and must fire once, not once per node that
        // walks into it.
        let mut package = ghost_lobby();
        package.beliefs = vec![
            belief(1, &[], &[1]),
            belief(2, &[], &[1]),
            belief(3, &[], &[1]),
        ];
        package.beliefs[0].supersedes = Some(link(2, 1));
        package.beliefs[1].supersedes = Some(link(3, 1));
        package.beliefs[2].supersedes = Some(link(2, 1));

        let faults = validate(&package).unwrap_err();
        let cycle_faults: Vec<_> = faults
            .iter()
            .filter(|f| f.rule() == "supersession-acyclic")
            .collect();
        assert_eq!(
            cycle_faults.len(),
            1,
            "exactly one fault per cycle, not one per entering node: {cycle_faults:?}"
        );
        match cycle_faults[0] {
            PackageFault::SupersessionCycle(id) => {
                assert!(
                    *id == BeliefId(2) || *id == BeliefId(3),
                    "belief 1 is the lead-in, not on the cycle 2 -> 3 -> 2; got {id:?}"
                );
            }
            other => panic!("expected SupersessionCycle, got {other:?}"),
        }
    }

    #[test]
    fn the_supersession_walk_is_linear_not_quadratic_in_chain_length() {
        // A crafted linear chain 1 -> 2 -> ... -> n is the hostile input a
        // package loader must survive cheaply. Walking each start with a
        // fresh `seen` set costs O(n^2); a global visited map costs O(n).
        //
        // Asserted by RATIO, not an absolute wall-clock bound: a fixed
        // millisecond threshold is exactly the kind of assertion that flakes
        // under a slow/shared/loaded CI runner without indicating any real
        // regression (gitar review on #33). Timing a small chain and a
        // scaled-up chain and bounding their ratio is robust to absolute
        // runner speed, because it tests the shape of the scaling curve, not
        // its position: doubling n roughly doubles work for O(n) but roughly
        // quadruples it for O(n^2), and that ratio holds regardless of how
        // fast or slow the machine underneath it is.
        fn chain_package(n: u64) -> EpistemicPackage {
            let events = vec![event(1, 1, 1)];
            let mut beliefs = Vec::with_capacity(n as usize);
            for id in 1..=n {
                let mut b = belief(id, &[], &[1]);
                if id < n {
                    b.supersedes = Some(link(id + 1, 1));
                }
                beliefs.push(b);
            }
            EpistemicPackage {
                manifest: manifest(),
                events,
                beliefs,
            }
        }

        // Each size run several times and the minimum kept, to further
        // damp scheduler noise without weakening what the ratio proves.
        fn fastest_run(n: u64) -> std::time::Duration {
            let package = chain_package(n);
            (0..5)
                .map(|_| {
                    let start = std::time::Instant::now();
                    let _ = validate(&package);
                    start.elapsed()
                })
                .min()
                .expect("at least one run")
        }

        const SMALL: u64 = 1_000;
        const LARGE: u64 = 8_000; // 8x SMALL
        let small = fastest_run(SMALL);
        let large = fastest_run(LARGE);

        // O(n) work scaling 8x n gives ~8x time; O(n^2) gives ~64x. 20x is a
        // generous cutoff that comfortably separates the two without being
        // sensitive to noise at these sub-millisecond magnitudes.
        let ratio = large.as_secs_f64() / small.as_secs_f64().max(f64::EPSILON);
        assert!(
            ratio < 20.0,
            "scaling the chain {}x (n={SMALL} -> n={LARGE}) took {ratio:.1}x as long \
             ({small:?} -> {large:?}); this is the signature of an O(n^2) walk, \
             not the required O(n)",
            LARGE / SMALL,
        );
    }

    #[test]
    fn a_package_round_trips_through_serde() {
        let package = ghost_lobby();
        let json = serde_json::to_string(&package).unwrap();
        let back: EpistemicPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(package, back);
    }
}
