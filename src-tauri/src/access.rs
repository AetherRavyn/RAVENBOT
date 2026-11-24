//! Server-side role checks for shared workspaces.
//!
//! The Tauri IPC surface is large, so this gate is fail-closed: an owner may
//! invoke anything, an operator may invoke an explicit run/message allowlist
//! plus read-only commands, and a viewer may invoke only read-only commands.
//! A command not listed for the role is denied even if the UI forgot to hide
//! its button.

/// Explicitly safe for a viewer even though it does not match a read prefix.
const VIEWER_EXTRA: &[&str] = &[
    "account_mode",
    "ravenbot_account_mode",
    "get_current_account",
    "logout_account",
    "open_main_window",
    "show_attention_island_command",
    "computer_capabilities",
    "default_workspace_for",
    "summarize_workspace",
];

/// Read-shaped commands that still expose credentials or administrative state.
const VIEWER_DENY: &[&str] = &[
    "get_mcp_server_env",
    "reveal_vault_item",
    "get_attention_webhook",
    "get_attention_email",
    "get_service_status",
    "get_autostart_enabled",
    "get_remote_status",
    "list_remote_devices",
    "list_workspace_grants",
    "list_audit_entries",
    "export_audit_log",
    "verify_audit_export",
];

/// Operator-only actions: running and steering work inside the workspace.
const OPERATOR_EXTRA: &[&str] = &[
    "get_model_catalog",
    "get_configured_providers",
    "get_provider_base_url",
    "fetch_provider_models",
    "fetch_all_provider_models",
    "probe_ollama",
    "create_thread",
    "send_message",
    "steer_thread",
    "regenerate_message",
    "edit_and_resend",
    "execute_graph",
    "send_to_chatroom",
    "post_channel_message",
    "create_channel_task",
    "start_channel_task",
    "channel_task_action",
    "decide_approval",
    "answer_question",
    "cancel_run",
    "pause_run",
    "resume_run",
    "run_routine_now",
    "run_office_queue_item_next",
    "cancel_office_queue_item",
    "dismiss_office_queue_item",
    "compact_thread_context",
    "toggle_message_reaction",
    "set_bot_pinned",
    "set_bot_hidden",
    "mark_bot_read",
    "rename_thread",
    "delete_thread",
    "write_workspace_file",
    "fill_browser_from_vault",
    "send_attention_notification",
    "computer_takeover",
    "set_computer_takeover",
    "start_bot_desktop",
    "stop_bot_desktop",
    "browser_dock_open",
    "browser_dock_new_tab",
    "browser_dock_activate_tab",
    "browser_dock_close_tab",
    "browser_dock_reorder_tabs",
    "browser_dock_set_bounds",
    "browser_dock_navigate",
    "browser_dock_back",
    "browser_dock_forward",
    "browser_dock_reload",
    "browser_dock_hide",
    "browser_dock_destroy",
    "browser_dock_eval",
    "browser_dock_user_eval",
    "browser_dock_takeover",
    "browser_dock_set_takeover",
    "browser_dock_focus",
];

fn is_read_command(command: &str) -> bool {
    command.starts_with("get_")
        || command.starts_with("list_")
        || command.starts_with("search_")
        || command.starts_with("read_")
        || command.starts_with("browse_")
        || command.starts_with("file_change_")
        || command.starts_with("preview_")
}

pub fn role_allows(role: &str, command: &str) -> bool {
    match role {
        "owner" => true,
        "operator" => {
            OPERATOR_EXTRA.contains(&command)
                || VIEWER_EXTRA.contains(&command)
                || (is_read_command(command) && !VIEWER_DENY.contains(&command))
        }
        "viewer" => {
            VIEWER_EXTRA.contains(&command)
                || (is_read_command(command) && !VIEWER_DENY.contains(&command))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_can_invoke_everything() {
        assert!(role_allows("owner", "set_provider_api_key"));
        assert!(role_allows("owner", "send_message"));
        assert!(role_allows("owner", "delete_account"));
    }

    #[test]
    fn viewers_are_read_only_and_cannot_read_secrets() {
        assert!(role_allows("viewer", "list_messages"));
        assert!(role_allows("viewer", "list_mcp_event_triggers"));
        assert!(role_allows("viewer", "list_thread_shares"));
        assert!(!role_allows("viewer", "create_mcp_event_trigger"));
        assert!(!role_allows("viewer", "create_thread_share"));
        assert!(role_allows("viewer", "get_current_account"));
        assert!(role_allows("viewer", "read_workspace_file"));
        assert!(!role_allows("viewer", "send_message"));
        assert!(!role_allows("viewer", "create_bot"));
        assert!(!role_allows("viewer", "set_provider_api_key"));
        assert!(!role_allows("viewer", "get_mcp_server_env"));
        assert!(!role_allows("viewer", "reveal_vault_item"));
        assert!(!role_allows("viewer", "list_remote_devices"));
    }

    #[test]
    fn operators_can_run_work_but_not_administer_the_workspace() {
        assert!(role_allows("operator", "send_message"));
        assert!(role_allows("operator", "decide_approval"));
        assert!(role_allows("operator", "list_messages"));
        assert!(!role_allows("operator", "set_provider_api_key"));
        assert!(!role_allows("operator", "create_bot"));
        assert!(!role_allows("operator", "create_mcp_event_trigger"));
        assert!(!role_allows("operator", "set_bot_budget"));
        assert!(!role_allows("operator", "get_mcp_server_env"));
        assert!(!role_allows("operator", "unknown_future_command"));
    }

    #[test]
    fn an_unknown_role_is_denied() {
        assert!(!role_allows("guest", "list_messages"));
    }
}
