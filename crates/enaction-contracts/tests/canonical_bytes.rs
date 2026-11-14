// SPDX-License-Identifier: AGPL-3.0-or-later
//! Canonical bytes: RFC 8785 JCS over integer-only I-JSON (owner ruling Q1,
//! arm A; ADR-0024).
//!
//! Vector provenance. The JCS vectors below are transcribed from
//! `hyperpolymath/ijson-jcs` at commit
//! `1735d68768920f6f59667c9ba2c84769d8d45336` (`tests/rfc8785.rs`,
//! `interop/kyq/vectors/0[1-8]-canon-*`, `tests/fixtures/standards/good/`).
//! That repository was cloned read-only into a scratch directory outside this
//! one; no file of it is vendored here. The UTF-16 key-order vector is the
//! one RFC 8785 section 3.2.3 publishes. Vectors whose input is a non-integer
//! number are kept, but their expected result under this profile is a
//! refusal, because Q1 arm A admits integers only.

use enaction_contracts::canonical::{self, CanonicalFault, MAX_SAFE_INTEGER};
use enaction_contracts::{
    Belief, BeliefId, BeliefInterval, BeliefStatus, ContractVersion, DepthPolicy, EpistemicEvent,
    EpistemicPackage, EventId, EventKind, KERNEL_CONTRACT_VERSION, Mass, PackageManifest,
    ProfileRef, Proposition, Provenance, SeedPolicy, StatusPolicy, Subject, package_digest,
    validate_package,
};

fn canon(input: &str) -> Result<String, CanonicalFault> {
    canonical::canonicalize(input.as_bytes()).map(|b| String::from_utf8(b).unwrap())
}

fn refused(input: &str) -> &'static str {
    canon(input).expect_err(input).rule()
}

#[test]
fn canonical_bytes_match_jcs_vectors() {
    // (input, expected canonical output)
    let accepted: &[(&str, &str)] = &[
        // kyq 01: object members sort.
        (r#"{"b":1,"a":2}"#, r#"{"a":2,"b":1}"#),
        // kyq 03: string escapes are kept minimal.
        (r#"{"s":"a\"b\\c\nd"}"#, r#"{"s":"a\"b\\c\nd"}"#),
        // kyq 04: UTF-16 code-unit order, not locale order.
        (
            "{\"caf\u{e9}\":1,\"cafe\":2}",
            "{\"cafe\":2,\"caf\u{e9}\":1}",
        ),
        // kyq 05: the safe-integer boundary is admitted both ways.
        (
            "[9007199254740991,-9007199254740991]",
            "[9007199254740991,-9007199254740991]",
        ),
        // RFC 8785 section 3.2.3: UTF-16 order is not scalar order.
        (
            r#"{"\u20ac":"Euro Sign","\r":"Carriage Return","\ufb33":"Hebrew Letter Dalet With Dagesh","1":"One","\ud83d\ude00":"Emoji: Grinning Face","\u0080":"Control","\u00f6":"Latin Small Letter O With Diaeresis"}"#,
            "{\"\\r\":\"Carriage Return\",\"1\":\"One\",\"\u{0080}\":\"Control\",\"\u{00f6}\":\"Latin Small Letter O With Diaeresis\",\"\u{20ac}\":\"Euro Sign\",\"\u{1f600}\":\"Emoji: Grinning Face\",\"\u{fb33}\":\"Hebrew Letter Dalet With Dagesh\"}",
        ),
        // A supplementary key sorts before U+E000 in UTF-16; arrays keep order.
        (
            r#"[{"\ue000":1,"\ud800\udc00":2},3,1,2]"#,
            "[{\"\u{10000}\":2,\"\u{e000}\":1},3,1,2]",
        ),
        // No Unicode normalisation, no JSON-LD resolution.
        (
            r#"{"\u00e9":1,"e\u0301":2,"@context":"https://example.invalid/context"}"#,
            "{\"@context\":\"https://example.invalid/context\",\"e\u{0301}\":2,\"\u{e9}\":1}",
        ),
        // good/canonical.json, good/nested.json, good/safe-integer.json.
        (r#"{"a":1,"b":[true,null]}"#, r#"{"a":1,"b":[true,null]}"#),
        (
            r#"{"outer":{"a":{"inner":true},"z":1}}"#,
            r#"{"outer":{"a":{"inner":true},"z":1}}"#,
        ),
        ("9007199254740991", "9007199254740991"),
        // Insignificant whitespace goes; -0 is 0.
        (" { \"a\" : [ 1 , -0 ] } ", r#"{"a":[1,0]}"#),
    ];
    for (input, want) in accepted {
        assert_eq!(canon(input).as_deref(), Ok(*want), "input {input}");
        // Canonicalisation is idempotent.
        assert_eq!(canon(want).as_deref(), Ok(*want), "idempotence of {want}");
    }

    // Every escaped control survives and re-parses; a raw control is refused.
    for code in 0u32..32 {
        let escaped = format!(r#""\u{code:04x}""#);
        let out = canon(&escaped).unwrap();
        assert_eq!(canon(&out).unwrap(), out);
        let raw = format!("\"{}\"", char::from_u32(code).unwrap());
        assert_eq!(refused(&raw), "MalformedJson");
    }
    assert_eq!(canon(r#""\u001f\b\t""#).unwrap(), r#""\u001f\b\t""#);

    // kyq 06, 07, 08, and kyq 02 under the integer-only profile.
    assert_eq!(refused("[9007199254740993]"), "IntegerOutOfRange");
    assert_eq!(refused(r#"{"a":1,"a":2}"#), "DuplicateKey");
    assert_eq!(refused(r#"{"a":"#), "MalformedJson");
    assert_eq!(
        refused("[1e30,4.50,0.002,1e-7,0,1e21,1e20]"),
        "NonIntegerNumber"
    );
    assert_eq!(refused("1.0"), "NonIntegerNumber", "1.0 is not an integer");
    assert_eq!(refused("1e0"), "NonIntegerNumber");
    // Grammar, not tolerance.
    for bad in [
        "01",
        "+1",
        "[1,]",
        "{\"a\":1,}",
        "'a'",
        "NaN",
        "[1] x",
        "\u{feff}1",
        "",
    ] {
        assert_eq!(refused(bad), "MalformedJson", "{bad:?}");
    }
    // I-JSON: a lone surrogate is not a string.
    assert_eq!(refused(r#""\ud800""#), "MalformedJson");
    assert_eq!(refused(r#""\udc00x""#), "MalformedJson");
}

#[test]
fn u64_boundaries_follow_q1() {
    assert_eq!(MAX_SAFE_INTEGER, (1u64 << 53) - 1);
    // Text.
    assert_eq!(canon("9007199254740991").unwrap(), "9007199254740991");
    assert_eq!(refused("9007199254740992"), "IntegerOutOfRange");
    assert_eq!(refused(&u64::MAX.to_string()), "IntegerOutOfRange");
    assert_eq!(refused("-9007199254740992"), "IntegerOutOfRange");
    assert_eq!(
        refused("123456789012345678901234567890"),
        "IntegerOutOfRange"
    );
    // Typed values on the way out.
    assert_eq!(
        canonical::canonical_bytes(&MAX_SAFE_INTEGER).unwrap(),
        b"9007199254740991"
    );
    let at = canonical::canonical_bytes(&(MAX_SAFE_INTEGER + 1)).unwrap_err();
    assert_eq!(at.rule(), "IntegerOutOfRange");
    let max = canonical::canonical_bytes(&u64::MAX).unwrap_err();
    assert_eq!(max.rule(), "IntegerOutOfRange");
    // Floats are refused on the way out too.
    assert_eq!(
        canonical::canonical_bytes(&0.5f64).unwrap_err().rule(),
        "NonIntegerNumber"
    );
}

#[test]
fn every_integer_field_is_bounded_including_max_order() {
    // A policy document is decoded through the same strict decoder.
    let ok: DepthPolicy = canonical::decode(br#"{"max_order":9007199254740991}"#).unwrap();
    assert_eq!(ok.max_order as u64, MAX_SAFE_INTEGER);
    let fault = canonical::decode::<DepthPolicy>(br#"{"max_order":9007199254740992}"#).unwrap_err();
    assert_eq!(fault.rule(), "IntegerOutOfRange");
    assert_eq!(fault.path(), "max_order");
}

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
            event_id: EventId(1),
            sequence: 1,
            tick: 1,
            observer: "guard".into(),
            subject: Subject::Agent("intruder".into()),
            kind: EventKind::Observation,
            proposition: Proposition("intruder in lobby".into()),
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
            bearer: "guard".into(),
            proposition: Proposition("intruder in lobby".into()),
            status: BeliefStatus::Believed,
            confidence: BeliefInterval::new(mass(8_000), mass(9_500)).unwrap(),
            source_events: vec![EventId(1)],
            valid_from: 1,
            supersedes: None,
        }],
    };
    p.manifest.digest = package_digest(&p).unwrap();
    p
}

fn validate(p: &EpistemicPackage) -> Result<(), Vec<enaction_contracts::PackageFault>> {
    validate_package(
        p,
        KERNEL_CONTRACT_VERSION,
        &DepthPolicy::default(),
        &policy(),
    )
}

#[test]
fn the_digest_is_sha256_hex_over_jcs_without_the_digest_field() {
    let p = package();
    assert_eq!(validate(&p), Ok(()));
    let d = &p.manifest.digest;
    assert_eq!(d.len(), 64, "SHA-256, hex");
    assert!(
        d.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    // The digest field is omitted from what it hashes: changing it does not
    // change the computed digest.
    let mut q = p.clone();
    q.manifest.digest = "anything".into();
    assert_eq!(package_digest(&q).unwrap(), *d);
    // And the projection is exactly JCS of the package minus that field.
    let mut v: serde_json::Value = serde_json::to_value(&p).unwrap();
    v["manifest"].as_object_mut().unwrap().remove("digest");
    let bytes = canonical::canonicalize(v.to_string().as_bytes()).unwrap();
    assert_eq!(*d, canonical::sha256_hex(&bytes));
}

#[test]
fn mutated_package_with_stale_digest_is_rejected() {
    let mut p = package();
    p.events[0].confidence = mass(8_001);
    let faults = validate(&p).unwrap_err();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].rule(), "digest-matches");
    assert_eq!(faults[0].path(), "manifest.digest");
}

#[test]
fn an_unrepresentable_integer_in_a_package_is_refused_by_path() {
    let mut p = package();
    p.events[0].tick = u64::MAX;
    let faults = validate(&p).unwrap_err();
    let f = faults
        .iter()
        .find(|f| f.rule() == "IntegerOutOfRange")
        .unwrap_or_else(|| panic!("{faults:?}"));
    assert_eq!(f.path(), "events[0].tick");
}

#[test]
fn certain_is_the_status_for_full_belief_on_the_wire() {
    // Rung 2: mass certainty is `certain`, never `known`.
    assert_eq!(
        policy().status(BeliefInterval::certain()),
        BeliefStatus::Certain
    );
    assert_eq!(
        serde_json::to_string(&BeliefStatus::Certain).unwrap(),
        "\"certain\""
    );
    assert!(serde_json::from_str::<BeliefStatus>("\"known\"").is_err());
}
