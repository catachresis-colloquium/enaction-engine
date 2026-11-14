// SPDX-License-Identifier: AGPL-3.0-or-later
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
    validate_package,
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

fn valid() -> EpistemicPackage {
    EpistemicPackage {
        manifest: manifest(),
        events: vec![event(1, 1, 1), event(2, 1, 2)],
        beliefs: vec![belief(1, &[], &[1]), belief(2, &["observer"], &[2])],
    }
}

/// The digest the caller computed. Stage 1 of the plan makes it mandatory;
/// the crate does not yet compute it.
const DIGEST: &str = "abc123";

fn validate(package: &EpistemicPackage) -> Result<(), Vec<PackageFault>> {
    validate_package(
        package,
        KERNEL_CONTRACT_VERSION,
        &DepthPolicy::default(),
        &policy(),
        DIGEST,
    )
}

/// A package whose declared digest matches what the caller computed.
fn sealed(mut package: EpistemicPackage) -> EpistemicPackage {
    package.manifest.digest = DIGEST.into();
    package
}

/// One planted package per rule, each sealed with a correct digest so that
/// only the planted rule (and rules it implies) fire.
fn planted() -> Vec<(&'static str, EpistemicPackage)> {
    let mut out = Vec::new();

    let mut p = valid();
    p.manifest.kernel_version = ContractVersion { major: 2, minor: 0 };
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
    p.beliefs[0].supersedes = Some(BeliefId(42));
    out.push(("supersedes-present", sealed(p)));

    let mut p = valid();
    p.beliefs[0].supersedes = Some(BeliefId(2));
    p.beliefs[1].supersedes = Some(BeliefId(1));
    out.push(("supersession-acyclic", sealed(p)));

    let mut p = valid();
    p.beliefs.push(belief(3, &[""], &[1]));
    out.push(("ascription-names-nonempty", sealed(p)));

    // The digest rule: the declared digest is not the computed one.
    let mut p = valid();
    p.manifest.digest = "stale".into();
    out.push(("digest-matches", p));

    // The status rule: the producer says Known, the interval says Believed.
    let mut p = valid();
    p.beliefs[0].status = BeliefStatus::Known;
    out.push(("status-agrees-with-interval", sealed(p)));

    out
}

#[test]
fn the_unplanted_package_is_valid_so_the_plants_are_the_cause() {
    assert_eq!(validate(&sealed(valid())), Ok(()));
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
        DIGEST,
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
