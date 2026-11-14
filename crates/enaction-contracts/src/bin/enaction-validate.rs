// SPDX-License-Identifier: AGPL-3.0-or-later
//! `enaction-validate`: admit or refuse a runner's output (CAC-KERNEL §9).
//!
//! ```text
//! enaction-validate --policies FILE < runner-output.json
//! ```
//!
//! * stdin: an `enaction-runner-output/1` envelope.
//! * `--policies FILE`: the caller's `enaction-policies/1` document. The
//!   status and depth policies come from the caller, never from the runner.
//!
//! Exit 0: valid; stdout is the envelope's canonical bytes and a newline.
//! Exit 1: a validation fault in the input; stdout has one
//! `<rule>\t<path>\t<detail>` line per fault.
//! Exit 2: an invocation or operational failure (a missing argument, an
//! unknown argument, an unreadable or invalid policies file, unreadable
//! stdin); a message on stderr, nothing on stdout.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::process::ExitCode;

use enaction_contracts::canonical;
use enaction_contracts::envelope::{Fault, parse_policies, validate_runner_output};

const USAGE: &str = "usage: enaction-validate --policies FILE < runner-output.json";

struct Args {
    policies: String,
}

fn parse_args() -> Result<Args, String> {
    let mut policies = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--policies" => {
                let value = args.next().ok_or("--policies needs a FILE")?;
                if policies.replace(value).is_some() {
                    return Err("--policies given twice".into());
                }
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Args {
        policies: policies.ok_or("--policies FILE is required")?,
    })
}

/// An invocation or operational failure.
fn fail(message: &str) -> ExitCode {
    eprintln!("enaction-validate: {message}\n{USAGE}");
    ExitCode::from(2)
}

fn refuse(faults: &[Fault]) -> ExitCode {
    let mut out = std::io::stdout().lock();
    for fault in faults {
        if writeln!(out, "{fault}").is_err() {
            return ExitCode::from(2);
        }
    }
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
    let mut input = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut input) {
        return fail(&format!("cannot read stdin: {e}"));
    }
    match validate_runner_output(&input, &policies) {
        Ok((_, value)) => {
            let mut bytes = canonical::to_jcs(&value);
            bytes.push(b'\n');
            if std::io::stdout().lock().write_all(&bytes).is_err() {
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        Err(faults) => refuse(&faults),
    }
}
