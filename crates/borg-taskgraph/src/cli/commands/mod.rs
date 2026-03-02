mod add_comment;
mod add_task_blocked_by;
mod add_task_labels;
mod add_task_reference;
mod approve_review;
mod clear_task_duplicate_of;
mod clear_task_parent;
mod create_task;
mod get_task;
mod list_comments;
mod list_duplicated_by;
mod list_events;
mod list_task_children;
mod next_task;
mod reassign_assignee;
mod reconcile_in_progress;
mod remove_task_blocked_by;
mod remove_task_labels;
mod remove_task_reference;
mod request_review_changes;
mod set_task_duplicate_of;
mod set_task_parent;
mod set_task_status;
mod split_task_into_subtasks;
mod submit_review;
mod update_task_fields;

pub struct CommandMapping {
    pub cli_name: &'static str,
    pub tool_name: &'static str,
}

pub fn all() -> &'static [CommandMapping] {
    &[
        create_task::MAPPING,
        get_task::MAPPING,
        update_task_fields::MAPPING,
        reassign_assignee::MAPPING,
        add_task_labels::MAPPING,
        remove_task_labels::MAPPING,
        set_task_parent::MAPPING,
        clear_task_parent::MAPPING,
        list_task_children::MAPPING,
        add_task_blocked_by::MAPPING,
        remove_task_blocked_by::MAPPING,
        set_task_duplicate_of::MAPPING,
        clear_task_duplicate_of::MAPPING,
        list_duplicated_by::MAPPING,
        add_task_reference::MAPPING,
        remove_task_reference::MAPPING,
        set_task_status::MAPPING,
        submit_review::MAPPING,
        approve_review::MAPPING,
        request_review_changes::MAPPING,
        split_task_into_subtasks::MAPPING,
        add_comment::MAPPING,
        list_comments::MAPPING,
        list_events::MAPPING,
        next_task::MAPPING,
        reconcile_in_progress::MAPPING,
    ]
}
