use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let sdk_dist = manifest_dir.join("../../packages/borg-agent-sdk/dist/borg-agent-sdk.min.js");
    let sdk_types = manifest_dir.join("../../packages/borg-agent-sdk/borg.d.ts");
    let out_file =
        Path::new(&env::var("OUT_DIR").expect("missing OUT_DIR")).join("borg_agent_sdk.bundle.js");
    let out_types_file =
        Path::new(&env::var("OUT_DIR").expect("missing OUT_DIR")).join("borg_agent_sdk.d.ts");

    println!("cargo:rerun-if-changed={}", sdk_dist.display());
    println!("cargo:rerun-if-changed={}", sdk_types.display());
    if !sdk_dist.exists() {
        panic!(
            "missing borg agent sdk bundle at {}. build it first with `bun run --cwd packages/borg-agent-sdk build`",
            sdk_dist.display()
        );
    }
    if !sdk_types.exists() {
        panic!(
            "missing borg agent sdk type definitions at {}",
            sdk_types.display()
        );
    }

    println!(
        "cargo:warning=using borg-agent-sdk bundle source: {}",
        sdk_dist.display()
    );
    let bundle = fs::read_to_string(&sdk_dist).expect("failed to read built sdk bundle");
    fs::write(out_file, bundle).expect("failed to write sdk bundle to OUT_DIR");

    let types = fs::read_to_string(&sdk_types).expect("failed to read sdk type definitions");
    fs::write(out_types_file, types).expect("failed to write sdk type definitions to OUT_DIR");
}
