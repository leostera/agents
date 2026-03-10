#[derive(Debug, Clone)]
pub struct Model {
    name: String,
}

impl Model {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}
