// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drives the built `enaction-validate` binary (WEEVIL-HOST-ROUTE A2).
//!
//! Exit 0: valid. Exit 1: a validation fault in the input, printed as one
//! `<rule>\t<path>\t<detail>` line per fault on stdout. Exit 2: an invocation
//! or operational failure, on stderr.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use enaction_contracts::{
    Belief, BeliefId, BeliefInterval, BeliefStatus, ContractVersion, EpistemicEvent,
    EpistemicPackage, EventId, EventKind, KERNEL_CONTRACT_VERSION, Mass, PackageManifest,
    ProfileRef, Proposition, Provenance, SeedPolicy, Subject, package_digest,
};

const BIN: &str = env!("CARGO_BIN_EXE_enaction-validate");

/// A path no other test (or test process) writes: tests run in parallel.
fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("validate_cli")
        .join(format!("{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

const POLICIES: &str = r#"{
  "contract": "enaction-policies/1",
  "status": {
    "id": "test/status/v1",
    "rejected_ceiling": 1000,
    "unknown_band": 6000,
    "believed_floor": 7000,
    "suspected_floor": 5000
  },
  "depth": {"max_order": 1}
}"#;

fn policies_file() -> PathBuf {
    let path = scratch("policies.json");
    std::fs::write(&path, POLICIES).unwrap();
    path
}

fn mass(v: u16) -> Mass {
    Mass::new(v).unwrap()
}

fn package() -> EpistemicPackage {
    let mut p = EpistemicPackage {
        manifest: PackageManifest {
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
        },
        events: vec![EpistemicEvent {
            event_id: EventId(17),
            sequence: 1,
            tick: 40,
            observer: "guard-0".into(),
            subject: Subject::Agent("contact-3".into()),
            kind: EventKind::Observation,
            proposition: Proposition("contact-3 in corridor".into()),
            confidence: mass(8_000),
            provenance: Provenance {
                origin: "sight".into(),
                channel: None,
            },
            affect: None,
            conation: None,
        }],
        beliefs: vec![Belief {
            belief_id: BeliefId(1),
            holders: vec![],
            bearer: "guard-0".into(),
            proposition: Proposition("contact-3 in corridor".into()),
            status: BeliefStatus::Believed,
            confidence: BeliefInterval::new(mass(8_000), mass(9_500)).unwrap(),
            source_events: vec![EventId(17)],
            valid_from: 40,
            supersedes: None,
        }],
    };
    p.manifest.digest = package_digest(&p).unwrap();
    p
}

fn envelope(package: &EpistemicPackage) -> serde_json::Value {
    serde_json::json!({
        "contract": "enaction-runner-output/1",
        "package": package,
        "world_truth": ["contact-3 in corridor"],
    })
}

fn run(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The binary may exit before reading stdin (exit 2); a broken pipe then
    // is expected, not a test failure.
    let _ = child.stdin.take().unwrap().write_all(stdin);
    child.wait_with_output().unwrap()
}

fn validate(stdin: &[u8]) -> Output {
    let policies = policies_file();
    run(&["--policies", policies.to_str().unwrap()], stdin)
}

/// The rule names printed on stdout, one per fault line.
fn rules(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split('\t').next().map(str::to_string))
        .collect()
}

fn show(out: &Output) -> String {
    format!(
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_valid_envelope_exits_0_and_passes_through_canonical_bytes() {
    let env = envelope(&package());
    let out = validate(env.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", show(&out));
    let canonical =
        enaction_contracts::canonical::canonicalize(env.to_string().as_bytes()).unwrap();
    let mut want = canonical;
    want.push(b'\n');
    assert_eq!(out.stdout, want, "stdout is the canonical envelope");
}

#[test]
fn an_empty_object_exits_1_with_an_envelope_fault() {
    let out = validate(b"{}\n");
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert_eq!(rules(&out), ["BadShape"], "{}", show(&out));
}

#[test]
fn a_mutated_package_with_a_stale_digest_exits_1() {
    let mut p = package();
    p.events[0].confidence = mass(8_001);
    let out = validate(envelope(&p).to_string().as_bytes());
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("digest-matches\tpackage.manifest.digest\t"),
        "{}",
        show(&out)
    );
    assert_eq!(rules(&out).len(), 1);
}

#[test]
fn a_missing_policies_file_exits_2() {
    let env = envelope(&package()).to_string();
    let missing = scratch("no-such-policies.json");
    let _ = std::fs::remove_file(&missing);
    let out = run(&["--policies", missing.to_str().unwrap()], env.as_bytes());
    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
    assert!(out.stdout.is_empty(), "nothing is admitted on exit 2");
    // No --policies at all is an invocation failure too.
    let out = run(&[], env.as_bytes());
    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
    // So is an argument the binary does not know.
    let policies = policies_file();
    let out = run(
        &["--policies", policies.to_str().unwrap(), "--frobnicate"],
        env.as_bytes(),
    );
    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
}

#[test]
fn a_malformed_policies_file_is_an_invocation_failure_not_a_bad_package() {
    let path = scratch("bad-policies.json");
    std::fs::write(&path, r#"{"contract":"enaction-policies/1"}"#).unwrap();
    let out = run(
        &["--policies", path.to_str().unwrap()],
        envelope(&package()).to_string().as_bytes(),
    );
    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
}

#[test]
fn unknown_fields_are_rejected_before_hashing() {
    // An extra member the producer sealed into its digest: if it were
    // dropped silently, the digest of the typed form would no longer cover
    // what the producer sent.
    let mut env = envelope(&package());
    env["package"]["manifest"]["comment"] = "trust me".into();
    let out = validate(env.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert_eq!(rules(&out), ["UnknownField"], "{}", show(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("UnknownField\tpackage.manifest.comment\t"),
        "{stdout}"
    );

    // Every nested contract type is strict, not only the top.
    let mut env = envelope(&package());
    env["package"]["events"][0]["provenance"]["note"] = 1.into();
    let out = validate(env.to_string().as_bytes());
    assert_eq!(rules(&out), ["UnknownField"], "{}", show(&out));
    let mut env = envelope(&package());
    env["package"]["beliefs"][0]["confidence"]["utility"] = 3.into();
    let out = validate(env.to_string().as_bytes());
    assert_eq!(rules(&out), ["UnknownField"], "{}", show(&out));
}

#[test]
fn runner_cannot_supply_its_own_policies() {
    let mut env = envelope(&package());
    env["policies"] = serde_json::from_str(POLICIES).unwrap();
    let out = validate(env.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert_eq!(rules(&out), ["UnknownField"], "{}", show(&out));
}

#[test]
fn the_contract_tag_must_match() {
    let mut env = envelope(&package());
    env["contract"] = "enaction-runner-output/2".into();
    let out = validate(env.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).starts_with("UnknownField\tcontract\t"),
        "{}",
        show(&out)
    );
}

#[test]
fn integers_and_floats_on_the_wire_follow_q1() {
    let env = envelope(&package()).to_string();
    let big = env.replace("\"tick\":40", "\"tick\":9007199254740992");
    assert_ne!(big, env);
    let out = validate(big.as_bytes());
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .starts_with("IntegerOutOfRange\tpackage.events[0].tick\t"),
        "{}",
        show(&out)
    );
    let float = env.replace("\"tick\":40", "\"tick\":40.0");
    let out = validate(float.as_bytes());
    assert_eq!(rules(&out), ["NonIntegerNumber"], "{}", show(&out));
}

#[test]
fn duplicate_truth_atoms_are_refused() {
    let mut env = envelope(&package());
    env["world_truth"] = serde_json::json!(["a", "a"]);
    let out = validate(env.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert_eq!(rules(&out), ["DuplicateTruthAtom"], "{}", show(&out));
}

#[test]
fn the_callers_status_policy_decides_not_the_package() {
    // The same package under a stricter policy: the producer's "believed"
    // no longer agrees with the interval.
    let strict = POLICIES.replace("\"believed_floor\": 7000", "\"believed_floor\": 9000");
    let path = scratch("strict-policies.json");
    std::fs::write(&path, strict).unwrap();
    let out = run(
        &["--policies", path.to_str().unwrap()],
        envelope(&package()).to_string().as_bytes(),
    );
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert_eq!(
        rules(&out),
        ["status-agrees-with-interval"],
        "{}",
        show(&out)
    );
}

fn mind_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/mind/v1")
}

fn run_mind(case: &Path) -> Output {
    let root = mind_root();
    let policies = root.join("policies.json");
    let trace = case.join("mind-trace.json");
    let runner = std::fs::read(root.join("runner-output.json")).unwrap();
    run(
        &[
            "--policies",
            policies.to_str().unwrap(),
            "--mind-trace",
            trace.to_str().unwrap(),
        ],
        &runner,
    )
}

#[test]
fn each_rejected_mind_case_exits_1_and_prints_exactly_its_rule() {
    let mut count = 0;
    for entry in std::fs::read_dir(mind_root().join("rejected")).unwrap() {
        let case = entry.unwrap().path();
        let rule = case.file_name().unwrap().to_str().unwrap().to_string();
        let out = run_mind(&case);
        assert_eq!(out.status.code(), Some(1), "{rule}: {}", show(&out));
        assert_eq!(rules(&out), [rule.as_str()], "{rule}: {}", show(&out));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.split('\t').nth(1).unwrap().starts_with("mind_trace"),
            "{rule}: the path names the mind trace: {stdout}"
        );
        count += 1;
    }
    assert_eq!(count, 10, "one case for each of the ten rules");
}

#[test]
fn each_accepted_mind_case_exits_0_with_both_canonical_documents() {
    for name in ["the-grudge", "the-flinch"] {
        let case = mind_root().join("accepted").join(name);
        let out = run_mind(&case);
        assert_eq!(out.status.code(), Some(0), "{name}: {}", show(&out));
        let lines: Vec<&[u8]> = out.stdout.split(|b| *b == b'\n').collect();
        assert_eq!(lines.len(), 3, "two lines and a final newline");
        let trace = std::fs::read(case.join("mind-trace.json")).unwrap();
        assert_eq!(
            lines[1],
            enaction_contracts::canonical::canonicalize(&trace).unwrap()
        );
    }
}

#[test]
fn a_missing_mind_trace_file_exits_2() {
    let root = mind_root();
    let policies = root.join("policies.json");
    let missing = scratch("no-such-mind-trace.json");
    let runner = std::fs::read(root.join("runner-output.json")).unwrap();
    let out = run(
        &[
            "--policies",
            policies.to_str().unwrap(),
            "--mind-trace",
            missing.to_str().unwrap(),
        ],
        &runner,
    );
    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
}
