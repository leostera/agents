#[cfg(feature = "apple")]
use std::env;

#[cfg(feature = "apple")]
use swift_rs::SwiftLinker;

fn main() {
    println!("cargo:rerun-if-changed=swift");
    #[cfg(feature = "apple")]
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        SwiftLinker::new("10.15")
            .with_package("AgentsLLMApple", "swift")
            .link();
    }
}
