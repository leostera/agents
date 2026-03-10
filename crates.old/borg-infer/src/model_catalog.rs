use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardcodedModel {
    pub model_id: &'static str,
    pub gguf_path: &'static str,
}

const HARDCODED_MODELS: &[HardcodedModel] = &[
    HardcodedModel {
        model_id: "local/default",
        gguf_path: "/tmp/model.gguf",
    },
    HardcodedModel {
        model_id: "local/llama-3.1-8b-q4",
        gguf_path: "/tmp/llama-3.1-8b-q4.gguf",
    },
];

pub fn hardcoded_models() -> &'static [HardcodedModel] {
    HARDCODED_MODELS
}

pub fn hardcoded_model_path(model_id: &str) -> Option<PathBuf> {
    let model_id = model_id.trim();
    HARDCODED_MODELS
        .iter()
        .find(|entry| entry.model_id == model_id)
        .map(|entry| PathBuf::from(entry.gguf_path))
}
