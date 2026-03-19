#[cfg(not(target_arch = "wasm32"))]
evals::setup!();

mod echo;

#[cfg(target_arch = "wasm32")]
mod worker_runtime;
