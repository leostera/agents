#[cfg(target_os = "macos")]
use swift_rs::SwiftLinker;

fn main() {
    println!("cargo:rerun-if-changed=swift");
    #[cfg(target_os = "macos")]
    {
        SwiftLinker::new("10.15")
            .with_package("AgentsLLMApple", "swift")
            .link();
    }
}
