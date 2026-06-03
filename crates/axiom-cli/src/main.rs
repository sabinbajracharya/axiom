//! The `axiom` compiler driver binary. A thin shell: collect args, hand them to
//! [`axiom_cli::run`], return its process exit code. All real logic (and its
//! tests) live in the library so they can be exercised without a subprocess.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    axiom_cli::run(&args)
}
