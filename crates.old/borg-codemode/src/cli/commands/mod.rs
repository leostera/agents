mod execute_code;
mod search_apis;

pub struct CommandMapping {
    pub cli_name: &'static str,
    pub tool_name: &'static str,
}

pub fn all() -> &'static [CommandMapping] {
    &[execute_code::MAPPING, search_apis::MAPPING]
}
