// SPDX-License-Identifier: AGPL-3.0-or-later
//! The frozen 1.x vectors (ADR-0030, "Names tell the truth").
//!
//! `conformance/tom/v1` and `conformance/mind/v1` hold kernel 1.0 packages
//! in `enaction-runner-output/1` envelopes. No file in them changes again.
//! This test does two things.
//!
//! * The freeze is enforced, not assumed: each file still matches its own
//!   `SHA256SUMS`, the file set is the one listed, and `SHA256SUMS` itself
//!   matches a digest pinned here. Editing a frozen file, adding one or
//!   removing one fails this test; so does regenerating `SHA256SUMS` to hide
//!   it. These vectors are never blessed (`ENACTION_BLESS` does not touch
//!   them).
//! * The old vectors are a regression test: a 2.0 host refuses every package
//!   in them, accepted and rejected. A package without a supersession link is
//!   refused by the tag rule and by `kernel-version-compatible`; one with a
//!   link (a bare belief id, the 1.x form) fails to decode, because the link
//!   is now a record that names its cause (ADR-0030, stage B).

use std::path::{Path, PathBuf};

use enaction_contracts::canonical::sha256_hex;
use enaction_contracts::{Policies, parse_policies, validate_runner_output};

/// SHA-256 of `conformance/tom/v1/SHA256SUMS` as frozen at 77cfdd6.
const TOM_V1_SUMS_DIGEST: &str = "4d09a3ccd08561b346f1ef80a6e1d6d987e28df90cf5c781fe9c1191f7501cfc";
/// SHA-256 of `conformance/mind/v1/SHA256SUMS` as frozen at 77cfdd6.
const MIND_V1_SUMS_DIGEST: &str =
    "089f67879ec06923ee576546def7491beb524558b7d7309cc1de6b55071b91bd";

fn corpus(family: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance")
        .join(family)
        .join("v1")
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Every way `root` can differ from its own `SHA256SUMS`: a listed file
/// that is missing or changed, a file that is not listed, a malformed line.
/// Empty when the frozen set is intact.
fn freeze_breaks(root: &Path) -> Vec<String> {
    let listing = String::from_utf8(read(&root.join("SHA256SUMS"))).expect("UTF-8 SHA256SUMS");
    let mut breaks = Vec::new();
    let mut listed = Vec::new();
    for line in listing.lines() {
        let Some((hex, name)) = line.split_once("  ") else {
            breaks.push(format!("malformed line {line:?}"));
            continue;
        };
        listed.push(name.to_string());
        match std::fs::read(root.join(name)) {
            Err(_) => breaks.push(format!("{name}: listed but missing")),
            Ok(bytes) if sha256_hex(&bytes) != hex => breaks.push(format!("{name}: changed")),
            Ok(_) => {}
        }
    }
    let mut present = Vec::new();
    files(root, &mut present);
    for path in present {
        let name = path
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        if name != "SHA256SUMS" && !listed.contains(&name) {
            breaks.push(format!("{name}: present but not listed"));
        }
    }
    breaks
}

#[test]
fn the_frozen_v1_corpora_still_match_their_own_sums() {
    for (family, pinned) in [("tom", TOM_V1_SUMS_DIGEST), ("mind", MIND_V1_SUMS_DIGEST)] {
        let root = corpus(family);
        assert_eq!(
            freeze_breaks(&root),
            Vec::<String>::new(),
            "{family}/v1 is frozen (ADR-0030): no file in it changes again"
        );
        assert_eq!(
            sha256_hex(&read(&root.join("SHA256SUMS"))),
            pinned,
            "{family}/v1/SHA256SUMS is frozen too: it matches the digest pinned in this test"
        );
    }
}

/// Every package in the frozen corpora: `(label, rule the case is named
/// for (rejected cases only), policies, envelope bytes)`.
fn frozen_packages() -> Vec<(String, Option<String>, Policies, Vec<u8>)> {
    let mut out = Vec::new();

    let tom = corpus("tom");
    let tom_policies = tom.join("policies.json");
    for kind in ["accepted", "rejected"] {
        let mut cases: Vec<_> = std::fs::read_dir(tom.join(kind))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        cases.sort();
        for case in cases {
            let name = case.file_name().unwrap().to_str().unwrap().to_string();
            // A case's own policies win over the corpus file.
            let own = case.join("policies.json");
            let policies = if own.exists() {
                own
            } else {
                tom_policies.clone()
            };
            out.push((
                format!("tom/v1/{kind}/{name}"),
                (kind == "rejected").then_some(name),
                parse_policies(&read(&policies)).unwrap(),
                read(&case.join("runner-output.json")),
            ));
        }
    }

    // The mind corpus has one paired runner output; its traces (accepted and
    // rejected) are all checked against it.
    let mind = corpus("mind");
    out.push((
        "mind/v1/runner-output.json".to_string(),
        None,
        parse_policies(&read(&mind.join("policies.json"))).unwrap(),
        read(&mind.join("runner-output.json")),
    ));
    out
}

/// What a 2.0 host reports for a frozen package, as `(rule, path)` pairs.
fn refusal(policies: &Policies, bytes: &[u8]) -> Vec<(String, String)> {
    match validate_runner_output(bytes, policies) {
        Ok(_) => panic!("a 2.0 host accepted a frozen 1.x package"),
        Err(faults) => faults
            .into_iter()
            .map(|f| (f.rule.to_string(), f.path))
            .collect(),
    }
}

fn pair(rule: &str, path: &str) -> (String, String) {
    (rule.to_string(), path.to_string())
}

/// The frozen cases that carry a 1.x supersession link (a bare belief id),
/// with the path of the first one. Stage B of ADR-0030 changed the link to a
/// record of `belief` and `because`, so the decoder refuses these before the
/// tag or the version is ever read. ADR-0030 "Limits" says so: a 1.x package
/// with a link fails to decode; one without a link is refused by rule.
const LINKED: [(&str, &str); 4] = [
    ("tom/v1/accepted/caught", "package.beliefs[2].supersedes"),
    (
        "tom/v1/accepted/false-exposure",
        "package.beliefs[2].supersedes",
    ),
    (
        "tom/v1/accepted/false-exposure-restated",
        "package.beliefs[2].supersedes",
    ),
    (
        "tom/v1/rejected/supersession-monotone",
        "package.beliefs[1].supersedes",
    ),
];

#[test]
fn a_2_0_host_refuses_every_package_in_the_frozen_v1_corpora() {
    let packages = frozen_packages();
    // The walk found what is frozen: 7 accepted and 3 rejected theory-of-mind
    // cases and the one paired mind runner output. A directory that went
    // missing would otherwise shrink this test silently.
    assert_eq!(
        packages.len(),
        7 + 3 + 1,
        "{:?}",
        packages.iter().map(|p| &p.0).collect::<Vec<_>>()
    );

    // Both refusals a 1.x envelope without a link earns: the tag, then the
    // version.
    let tag = pair("UnknownField", "contract");
    let version = pair(
        "kernel-version-compatible",
        "package.manifest.kernel_version",
    );

    let mut linked = Vec::new();
    for (label, rejected_for, policies, bytes) in packages {
        let found = refusal(&policies, &bytes);
        eprintln!("{label}: {found:?}");
        if let Some((_, path)) = LINKED.iter().find(|(l, _)| *l == label) {
            // Refused at decode, as one fault, whatever else the case holds:
            // its own rule is never reached (the rejected case
            // `supersession-monotone` names a rule that no longer exists).
            assert_eq!(found, [pair("BadShape", path)], "{label}");
            linked.push(label);
            continue;
        }
        match rejected_for {
            // An accepted 1.x case, or the mind runner output, is refused by
            // those two rules and by nothing else.
            None => assert_eq!(found, [tag.clone(), version.clone()], "{label}"),
            // A rejected 1.x case is still refused for its own rule too. The
            // 2.0 host adds the tag and the version in front of it.
            Some(rule) => {
                assert_eq!(&found[..2], [tag.clone(), version.clone()], "{label}");
                assert_eq!(found.len(), 3, "{label}: {found:?}");
                assert_eq!(found[2].0, rule, "{label}: its own rule is still reported");
            }
        }
    }
    // Exactly the cases that carry a link were refused by shape.
    assert_eq!(
        linked,
        LINKED
            .iter()
            .map(|(l, _)| (*l).to_string())
            .collect::<Vec<_>>()
    );
}
