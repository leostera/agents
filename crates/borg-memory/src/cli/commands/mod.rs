mod create_entity;
mod get_entity;
mod get_schema;
mod list_facts;
mod new_entity;
mod retract_facts;
mod save_facts;
mod schema_define_field;
mod schema_define_kind;
mod schema_define_namespace;
mod search;
mod search_memory;
mod state_facts;

pub struct CommandMapping {
    pub cli_name: &'static str,
    pub tool_name: &'static str,
}

pub fn all() -> &'static [CommandMapping] {
    &[
        state_facts::MAPPING,
        search::MAPPING,
        create_entity::MAPPING,
        get_entity::MAPPING,
        retract_facts::MAPPING,
        list_facts::MAPPING,
        schema_define_namespace::MAPPING,
        schema_define_kind::MAPPING,
        schema_define_field::MAPPING,
        get_schema::MAPPING,
        new_entity::MAPPING,
        save_facts::MAPPING,
        search_memory::MAPPING,
    ]
}
