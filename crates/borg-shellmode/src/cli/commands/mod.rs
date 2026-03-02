mod execute_command;

pub struct CommandMapping {
    pub cli_name: &'static str,
    pub tool_name: &'static str,
}

pub fn all() -> &'static [CommandMapping] {
    &[execute_command::MAPPING]
}
