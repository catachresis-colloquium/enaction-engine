// SPDX-License-Identifier: MPL-2.0
//! Every package rule fires on a planted package (completion-plan step 3).
//!
//! A rule that no planted package can trip is not a rule. This file plants
//! one package per rule, validates it, and requires that the set of rules
//! observed firing equals [`PackageFault::RULES`] exactly. A unit test inside
//! the crate pins `RULES` to the variants by an exhaustive match, so a new
//! fault cannot be added without either a planted package here or a failing
//! test.

use std::collections::BTreeSet;

use enaction_contracts::{
    Belief, BeliefId, BeliefInterval, BeliefStatus, ContractVersion, DepthPolicy, EpistemicEvent,
    EpistemicPackage, EventId, EventKind, KERNEL_CONTRACT_VERSION, Mass, PackageFault,
    PackageManifest, ProfileRef, Proposition, Provenance, SeedPolicy, StatusPolicy, Subject,
    Supersession, validate_package,
};

fn mass(v: u16) -> Mass {
    Mass::new(v).unwrap()
}

fn policy() -> StatusPolicy {
    StatusPolicy {
        id: "test/status/v1".into(),
        rejected_ceiling: mass(1_000),
        unknown_band: mass(6_000),
        believed_floor: mass(7_000),
        suspected_floor: mass(5_000),
    }
}

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
        digest: String::new(),
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
        confidence: mass(8_000),
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
        // (8000, 9500) under `policy()` reads Believed: ignorance 1500 is
        // under the 6000 band, belief clears the 7000 floor.
        status: BeliefStatus::Believed,
        confidence: BeliefInterval::new(mass(8_000), mass(9_500)).unwrap(),
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

fn valid() -> EpistemicPackage {
    EpistemicPackage {
        manifest: manifest(),
        events: vec![event(1, 1, 1), event(2, 1, 2)],
        beliefs: vec![belief(1, &[], &[1]), belief(2, &["observer"], &[2])],
    }
}

fn validate(package: &EpistemicPackage) -> Result<(), Vec<PackageFault>> {
    validate_package(
        package,
        KERNEL_CONTRACT_VERSION,
        &DepthPolicy::default(),
        &policy(),
    )
}

/// A package whose declared digest is right for it as it stands. The
/// validator computes the digest itself; no caller supplies one.
fn sealed(mut package: EpistemicPackage) -> EpistemicPackage {
    package.manifest.digest = enaction_contracts::package_digest(&package).unwrap();
    package
}

/// What is wrong with the one link of [`cause_plant`].
#[derive(Clone, Copy, Debug)]
enum Plant {
    /// Nothing: the lawful link.
    Lawful,
    /// The cause is an event the package does not have.
    Absent,
    /// The cause reached the intruder, not the guard.
    SeenByOther,
    /// The successor rests on event 2, not on its cause.
    NotCited,
    /// The replaced belief already rests on the cause.
    Stale,
    /// The replaced belief starts on tick 4, after the cause (tick 3).
    BeforeReplaced,
    /// The successor starts on tick 2, before the cause (tick 3).
    AfterSuccessor,
}

/// `valid()` plus event 3 (the guard's, tick 3), event 4 (the intruder's,
/// tick 3) and belief 3, the guard's own belief replacing belief 1 because of
/// event 3. Belief 3 starts on tick 3 and rests on event 3; belief 1 starts on
/// tick 1 and rests on event 1. That link is lawful; `plant` breaks one thing.
fn cause_plant(plant: Plant) -> EpistemicPackage {
    let mut p = valid();
    let mut intruder = event(4, 3, 2);
    intruder.observer = "intruder".into();
    p.events.push(event(3, 3, 1));
    p.events.push(intruder);
    let mut successor = belief(3, &[], &[3]);
    successor.valid_from = 3;
    successor.supersedes = Some(link(1, 3));
    match plant {
        Plant::Lawful => {}
        Plant::Absent => successor.supersedes = Some(link(1, 99)),
        Plant::SeenByOther => {
            successor.supersedes = Some(link(1, 4));
            successor.source_events = vec![EventId(4)];
        }
        Plant::NotCited => successor.source_events = vec![EventId(2)],
        Plant::Stale => p.beliefs[0].source_events = vec![EventId(1), EventId(3)],
        Plant::BeforeReplaced => {
            p.beliefs[0].valid_from = 4;
            successor.valid_from = 4;
        }
        Plant::AfterSuccessor => successor.valid_from = 2,
    }
    p.beliefs.push(successor);
    p
}

/// One planted package per rule, each sealed with a correct digest so that
/// only the planted rule (and rules it implies) fire.
fn planted() -> Vec<(&'static str, EpistemicPackage)> {
    let mut out = Vec::new();

    // A 1.x package under the 2.0 host (ADR-0030).
    let mut p = valid();
    p.manifest.kernel_version = ContractVersion { major: 1, minor: 0 };
    out.push(("kernel-version-compatible", sealed(p)));

    let mut p = valid();
    p.events.push(event(1, 2, 1));
    out.push(("event-id-unique", sealed(p)));

    let mut p = valid();
    p.beliefs.push(belief(1, &[], &[1]));
    out.push(("belief-id-unique", sealed(p)));

    let mut p = valid();
    p.events = vec![event(1, 2, 1), event(2, 1, 1)];
    out.push(("events-monotonic", sealed(p)));

    let mut p = valid();
    p.beliefs.push(belief(3, &["a", "b"], &[1]));
    out.push(("nesting-bounded", sealed(p)));

    let mut p = valid();
    p.beliefs.push(belief(3, &[], &[]));
    out.push(("belief-has-provenance", sealed(p)));

    let mut p = valid();
    p.beliefs.push(belief(3, &[], &[99]));
    out.push(("source-events-present", sealed(p)));

    let mut p = valid();
    p.beliefs[0].supersedes = Some(link(42, 1));
    out.push(("supersedes-present", sealed(p)));

    let mut p = valid();
    p.beliefs[0].supersedes = Some(link(2, 1));
    p.beliefs[1].supersedes = Some(link(1, 1));
    out.push(("supersession-acyclic", sealed(p)));

    // A silent overwrite (CAC-KERNEL §3 invariant 4, ADR-0029): a second
    // belief of the guard about the same proposition that supersedes nothing.
    let mut p = valid();
    let mut later = belief(3, &[], &[2]);
    later.valid_from = 2;
    p.beliefs.push(later);
    out.push(("belief-key-chained", sealed(p)));

    // ADR-0030: the cause of a link, one plant per rule. Each is the lawful
    // link of `cause_plant` with one thing changed, so that it trips its own
    // cause rule and no other
    // (`the_lawful_link_is_accepted_and_each_cause_plant_trips_only_its_rule`).
    out.push((
        "supersession-cause-present",
        sealed(cause_plant(Plant::Absent)),
    ));
    out.push((
        "supersession-cause-observed",
        sealed(cause_plant(Plant::SeenByOther)),
    ));
    out.push((
        "supersession-cause-cited",
        sealed(cause_plant(Plant::NotCited)),
    ));
    out.push(("supersession-cause-new", sealed(cause_plant(Plant::Stale))));
    out.push((
        "supersession-cause-not-earlier",
        sealed(cause_plant(Plant::BeforeReplaced)),
    ));
    out.push((
        "supersession-cause-not-later",
        sealed(cause_plant(Plant::AfterSuccessor)),
    ));

    let mut p = valid();
    p.beliefs.push(belief(3, &[""], &[1]));
    out.push(("ascription-names-nonempty", sealed(p)));

    // The digest rule: a sealed package mutated after sealing.
    let mut p = sealed(valid());
    p.manifest.source = "ums:tampered".into();
    out.push(("digest-matches", p));

    // Q1: an integer above 2^53-1 has no canonical bytes, hence no digest.
    let mut p = valid();
    p.events[1].sequence = u64::MAX;
    out.push(("IntegerOutOfRange", p));

    // The status rule: the producer says Certain, the interval says Believed.
    let mut p = valid();
    p.beliefs[0].status = BeliefStatus::Certain;
    out.push(("status-agrees-with-interval", sealed(p)));

    out
}

#[test]
fn the_unplanted_package_is_valid_so_the_plants_are_the_cause() {
    assert_eq!(validate(&sealed(valid())), Ok(()));
}

#[test]
fn the_lawful_link_is_accepted_and_each_cause_plant_trips_only_its_rule() {
    assert_eq!(validate(&sealed(cause_plant(Plant::Lawful))), Ok(()));
    for (plant, rule) in [
        (Plant::Absent, "supersession-cause-present"),
        (Plant::SeenByOther, "supersession-cause-observed"),
        (Plant::NotCited, "supersession-cause-cited"),
        (Plant::Stale, "supersession-cause-new"),
        (Plant::BeforeReplaced, "supersession-cause-not-earlier"),
        (Plant::AfterSuccessor, "supersession-cause-not-later"),
    ] {
        let faults = validate(&sealed(cause_plant(plant))).unwrap_err();
        let rules: Vec<_> = faults.iter().map(PackageFault::rule).collect();
        assert_eq!(rules, [rule], "{plant:?}: {faults:?}");
        assert_eq!(
            faults[0].path(),
            "beliefs[belief_id=3].supersedes.because",
            "{plant:?}"
        );
    }
}

#[test]
fn every_rule_fires_on_its_planted_package() {
    let mut fired = BTreeSet::new();
    for (rule, package) in planted() {
        let faults = validate(&package).expect_err(rule);
        let rules: BTreeSet<_> = faults.iter().map(PackageFault::rule).collect();
        assert!(
            rules.contains(rule),
            "planted {rule} but saw {rules:?}: {faults:?}"
        );
        for fault in &faults {
            assert!(!fault.path().is_empty(), "every fault names a field path");
        }
        fired.insert(rule);
    }
    let all: BTreeSet<_> = PackageFault::RULES.iter().copied().collect();
    assert_eq!(fired, all, "every rule has a planted package, and no more");
}

#[test]
fn the_status_check_uses_the_callers_policy_not_the_producers_word() {
    // The same interval reads differently under two profile policies. A
    // producer cannot pick: the caller's policy decides.
    let package = sealed(valid());
    let strict = StatusPolicy {
        believed_floor: mass(9_000),
        ..policy()
    };
    let faults = validate_package(
        &package,
        KERNEL_CONTRACT_VERSION,
        &DepthPolicy::default(),
        &strict,
    )
    .unwrap_err();
    assert!(
        faults
            .iter()
            .any(|f| f.rule() == "status-agrees-with-interval"),
        "{faults:?}"
    );
    assert_eq!(
        faults[0].path(),
        "beliefs[belief_id=1].status",
        "the fault names the status field"
    );
}
