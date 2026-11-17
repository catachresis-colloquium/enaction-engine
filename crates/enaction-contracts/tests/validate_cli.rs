// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drives the built `enaction-validate` binary (WEEVIL-HOST-ROUTE A2).
//!
//! Exit 0: valid. Exit 1: a validation fault in the input; stdout is exactly
//! one line, the canonical JSON object
//! `{"faults":[{"detail":..,"path":..,"rule":..}]}`, and stderr repeats each
//! fault as a `<rule>\t<path>\t<detail>` line for a human. Exit 2: an
//! invocation or operational failure, on stderr.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use enaction_contracts::{
    Belief, BeliefId, BeliefInterval, BeliefStatus, ContractVersion, EpistemicEvent,
    EpistemicPackage, EventId, EventKind, KERNEL_CONTRACT_STATE, KERNEL_CONTRACT_VERSION, Mass,
    PackageManifest, ProfileRef, Proposition, Provenance, SeedPolicy, Subject, package_digest,
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
        "contract": "enaction-runner-output/2",
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

/// The `(rule, path)` of every fault in the one-line JSON refusal on stdout.
fn faults(out: &Output) -> Vec<(String, String)> {
    let stdout = std::str::from_utf8(&out.stdout).expect("UTF-8 stdout");
    let line = stdout
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("one line ending in a newline: {stdout:?}"));
    assert!(!line.contains('\n'), "exactly one line: {stdout:?}");
    let value: serde_json::Value = serde_json::from_str(line).expect("JSON");
    let object = value.as_object().expect("an object");
    assert_eq!(object.len(), 1, "only `faults`: {line}");
    value["faults"]
        .as_array()
        .expect("faults is an array")
        .iter()
        .map(|f| {
            let f = f.as_object().expect("a fault object");
            assert_eq!(f.len(), 3, "rule, path, detail: {f:?}");
            assert!(f["detail"].is_string());
            (
                f["rule"].as_str().unwrap().to_string(),
                f["path"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// The rule names of the refusal on stdout, in reported order.
fn rules(out: &Output) -> Vec<String> {
    faults(out).into_iter().map(|(rule, _)| rule).collect()
}

fn fault(rule: &str, path: &str) -> (String, String) {
    (rule.to_string(), path.to_string())
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
    assert_eq!(
        faults(&out),
        [fault("digest-matches", "package.manifest.digest")],
        "{}",
        show(&out)
    );
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
    assert_eq!(
        faults(&out),
        [fault("UnknownField", "package.manifest.comment")],
        "{}",
        show(&out)
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
    // The 1.x tag with a 2.0 body is refused at `contract`, by the tag rule
    // and by nothing else (ADR-0030). So is any other tag.
    for tag in [
        "enaction-runner-output/1",
        "enaction-runner-output/3",
        "enaction-runner-output",
    ] {
        let mut env = envelope(&package());
        env["contract"] = tag.into();
        let out = validate(env.to_string().as_bytes());
        assert_eq!(out.status.code(), Some(1), "{tag}: {}", show(&out));
        assert_eq!(
            faults(&out),
            [fault("UnknownField", "contract")],
            "{tag}: {}",
            show(&out)
        );
    }
}

#[test]
fn version_prints_the_contract_version_with_its_state() {
    let out = run(&["--version"], b"");
    assert_eq!(out.status.code(), Some(0), "{}", show(&out));
    let line = format!(
        "enaction-validate kernel contract {KERNEL_CONTRACT_VERSION} ({})\n",
        KERNEL_CONTRACT_STATE.as_str()
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), line);
    // ADR-0030: 2.0 is a draft, and the host says so next to the version.
    assert_eq!(line, "enaction-validate kernel contract 2.0 (draft)\n");
    // `--version` is read alone: anything with it is an invocation failure.
    let out = run(&["--version", "--frobnicate"], b"");
    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
    assert!(out.stdout.is_empty(), "nothing on stdout on exit 2");
}

#[test]
fn integers_and_floats_on_the_wire_follow_q1() {
    let env = envelope(&package()).to_string();
    let big = env.replace("\"tick\":40", "\"tick\":9007199254740992");
    assert_ne!(big, env);
    let out = validate(big.as_bytes());
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert_eq!(
        faults(&out),
        [fault("IntegerOutOfRange", "package.events[0].tick")],
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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/mind/v2")
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
        let (_, path) = &faults(&out)[0];
        assert!(
            path.starts_with("mind_trace"),
            "{rule}: the path names the mind trace: {path}"
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

#[test]
fn a_refusal_is_one_canonical_json_line_and_the_human_lines_go_to_stderr() {
    // Two faults at once: a stale digest and a repeated truth atom.
    let mut p = package();
    p.events[0].confidence = mass(8_001);
    let mut env = envelope(&p);
    env["world_truth"] = serde_json::json!(["a", "a"]);
    let out = validate(env.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert_eq!(
        faults(&out),
        [
            fault("digest-matches", "package.manifest.digest"),
            fault("DuplicateTruthAtom", "world_truth[1]"),
        ],
        "every fault, in reported order: {}",
        show(&out)
    );
    // Re-canonicalising the line gives the same bytes: it is canonical.
    let line = out.stdout.strip_suffix(b"\n").unwrap();
    assert_eq!(
        enaction_contracts::canonical::canonicalize(line).unwrap(),
        line,
        "stdout is canonical JCS"
    );
    // stderr carries the same faults as tab-separated lines, in order.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let human: Vec<_> = stderr
        .lines()
        .map(|l| {
            let mut parts = l.split('\t');
            (
                parts.next().unwrap().to_string(),
                parts.next().unwrap_or_default().to_string(),
            )
        })
        .collect();
    assert_eq!(human, faults(&out), "{stderr}");
}
