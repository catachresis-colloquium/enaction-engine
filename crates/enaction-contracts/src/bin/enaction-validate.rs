// SPDX-License-Identifier: AGPL-3.0-or-later
//! `enaction-validate`: admit or refuse a runner's output (CAC-KERNEL §9).
//!
//! ```text
//! enaction-validate --policies FILE [--mind-trace FILE] < runner-output.json
//! ```
//!
//! * stdin: an `enaction-runner-output/1` envelope.
//! * `--policies FILE`: the caller's `enaction-policies/1` document. The
//!   status and depth policies come from the caller, never from the runner.
//! * `--mind-trace FILE`: an `enaction-mind-trace/1` document, validated
//!   against the envelope on stdin (its causes must be the envelope's
//!   events). Its fault paths are prefixed `mind_trace`.
//!
//! Exit 0: valid; stdout is the envelope's canonical bytes and a newline,
//! then, with `--mind-trace`, the mind trace's canonical bytes and a newline.
//! Exit 1: a validation fault in the input. Stdout is exactly one line: the
//! canonical (RFC 8785) JSON object
//! `{"faults":[{"detail":"..","path":"..","rule":".."}]}`, every fault in
//! reported order. Stderr repeats each fault as a `<rule>\t<path>\t<detail>`
//! line for a human reader.
//! Exit 2: an invocation or operational failure (a missing argument, an
//! unknown argument, an unreadable or invalid policies file, an unreadable
//! mind-trace file, unreadable stdin); a message on stderr, nothing on stdout.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::process::ExitCode;

use enaction_contracts::canonical;
use enaction_contracts::envelope::{Fault, parse_policies, validate_runner_output};
use enaction_contracts::mind::validate_mind_trace;

const USAGE: &str =
    "usage: enaction-validate --policies FILE [--mind-trace FILE] < runner-output.json";

struct Args {
    policies: String,
    mind_trace: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut policies = None;
    let mut mind_trace = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--policies" => {
                let value = args.next().ok_or("--policies needs a FILE")?;
                if policies.replace(value).is_some() {
                    return Err("--policies given twice".into());
                }
            }
            "--mind-trace" => {
                let value = args.next().ok_or("--mind-trace needs a FILE")?;
                if mind_trace.replace(value).is_some() {
                    return Err("--mind-trace given twice".into());
                }
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Args {
        policies: policies.ok_or("--policies FILE is required")?,
        mind_trace,
    })
}

/// An invocation or operational failure.
fn fail(message: &str) -> ExitCode {
    eprintln!("enaction-validate: {message}\n{USAGE}");
    ExitCode::from(2)
}

fn refuse(faults: &[Fault]) -> ExitCode {
    let report = serde_json::json!({
        "faults": faults
            .iter()
            .map(|f| serde_json::json!({"rule": f.rule, "path": f.path, "detail": f.detail}))
            .collect::<Vec<_>>(),
    });
    // Strings only: canonicalisation cannot fail on this value.
    let Ok(mut line) = canonical::canonical_bytes(&report) else {
        return fail("cannot canonicalise the fault report");
    };
    line.push(b'\n');
    let mut human = String::new();
    for fault in faults {
        human.push_str(&format!("{fault}\n"));
    }
    if std::io::stdout().lock().write_all(&line).is_err() {
        return ExitCode::from(2);
    }
    // A human copy only; a closed stderr does not change the verdict.
    let _ = std::io::stderr().lock().write_all(human.as_bytes());
    ExitCode::from(1)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => return fail(&message),
    };
    let policies = match std::fs::read(&args.policies) {
        Ok(bytes) => bytes,
        Err(e) => return fail(&format!("cannot read --policies {:?}: {e}", args.policies)),
    };
    let policies = match parse_policies(&policies) {
        Ok(policies) => policies,
        Err(fault) => {
            return fail(&format!("invalid --policies {:?}: {fault}", args.policies));
        }
    };
    let mind_trace = match &args.mind_trace {
        None => None,
        Some(path) => match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) => return fail(&format!("cannot read --mind-trace {path:?}: {e}")),
        },
    };
    let mut input = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut input) {
        return fail(&format!("cannot read stdin: {e}"));
    }

    let mut faults = Vec::new();
    let mut out = Vec::new();
    match validate_runner_output(&input, &policies) {
        Ok((_, value)) => {
            out.extend(canonical::to_jcs(&value));
            out.push(b'\n');
        }
        Err(found) => faults.extend(found),
    }
    if let Some(trace) = mind_trace {
        // The mind trace is checked whenever the envelope can be read at all,
        // so one run reports the faults of both documents. If the envelope
        // cannot be decoded, its causes are unknown and the trace is not
        // checked; the envelope's own fault is already reported.
        if let Ok(runner) = canonical::decode(&input) {
            match validate_mind_trace(&trace, &runner) {
                Ok((_, value)) => {
                    out.extend(canonical::to_jcs(&value));
                    out.push(b'\n');
                }
                Err(found) => faults.extend(found.into_iter().map(|f| Fault {
                    path: if f.path == "$" {
                        "mind_trace".into()
                    } else {
                        format!("mind_trace.{}", f.path)
                    },
                    ..f
                })),
            }
        }
    }
    if !faults.is_empty() {
        return refuse(&faults);
    }
    if std::io::stdout().lock().write_all(&out).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
