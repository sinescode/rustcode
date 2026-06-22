use std::sync::OnceLock;

static GIT_BASH_PATH: OnceLock<Option<String>> = OnceLock::new();

pub fn git_bash_path() -> Option<&'static str> {
    GIT_BASH_PATH
        .get_or_init(|| {
            std::env::var("OPENCODE_GIT_BASH_PATH")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .as_deref()
}

pub fn set_git_bash_path(path: String) {
    let _ = GIT_BASH_PATH.set(Some(path));
}

pub fn truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn enabled_by_experimental(name: &str) -> bool {
    if std::env::var(name).is_ok() {
        truthy(name)
    } else {
        truthy("OPENCODE_EXPERIMENTAL")
    }
}

#[derive(Debug, Clone)]
pub struct Flags {
    pub otel_exporter_otlp_endpoint: Option<String>,
    pub otel_exporter_otlp_headers: Option<String>,
    pub opencode_auto_heap_snapshot: bool,
    pub opencode_git_bash_path: Option<String>,
    pub opencode_config: Option<String>,
    pub opencode_config_content: Option<String>,
    pub opencode_disable_autoupdate: bool,
    pub opencode_always_notify_update: bool,
    pub opencode_disable_prune: bool,
    pub opencode_disable_terminal_title: bool,
    pub opencode_show_ttfd: bool,
    pub opencode_disable_autocompact: bool,
    pub opencode_disable_models_fetch: bool,
    pub opencode_disable_mouse: bool,
    pub opencode_fake_vcs: Option<String>,
    pub opencode_server_password: Option<String>,
    pub opencode_server_username: Option<String>,
    pub opencode_disable_fff: bool,
    pub opencode_experimental_filewatcher: bool,
    pub opencode_experimental_disable_filewatcher: bool,
    pub opencode_experimental_disable_copy_on_select: bool,
    pub opencode_models_url: Option<String>,
    pub opencode_models_path: Option<String>,
    pub opencode_db: Option<String>,
    pub opencode_workspace_id: Option<String>,
    pub opencode_experimental_workspaces: bool,
    pub opencode_disable_project_config: bool,
    pub opencode_experimental_references: bool,
    pub opencode_tui_config: Option<String>,
    pub opencode_config_dir: Option<String>,
    pub opencode_pure: bool,
    pub opencode_permission: Option<String>,
    pub opencode_plugin_meta_file: Option<String>,
    pub opencode_client: String,
}

impl Flags {
    pub fn from_env() -> Self {
        let fff = std::env::var("OPENCODE_DISABLE_FFF").ok();
        let copy = std::env::var("OPENCODE_EXPERIMENTAL_DISABLE_COPY_ON_SELECT").ok();
        Self {
            otel_exporter_otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            otel_exporter_otlp_headers: std::env::var("OTEL_EXPORTER_OTLP_HEADERS").ok(),
            opencode_auto_heap_snapshot: truthy("OPENCODE_AUTO_HEAP_SNAPSHOT"),
            opencode_git_bash_path: std::env::var("OPENCODE_GIT_BASH_PATH").ok(),
            opencode_config: std::env::var("OPENCODE_CONFIG").ok(),
            opencode_config_content: std::env::var("OPENCODE_CONFIG_CONTENT").ok(),
            opencode_disable_autoupdate: truthy("OPENCODE_DISABLE_AUTOUPDATE"),
            opencode_always_notify_update: truthy("OPENCODE_ALWAYS_NOTIFY_UPDATE"),
            opencode_disable_prune: truthy("OPENCODE_DISABLE_PRUNE"),
            opencode_disable_terminal_title: truthy("OPENCODE_DISABLE_TERMINAL_TITLE"),
            opencode_show_ttfd: truthy("OPENCODE_SHOW_TTFD"),
            opencode_disable_autocompact: truthy("OPENCODE_DISABLE_AUTOCOMPACT"),
            opencode_disable_models_fetch: truthy("OPENCODE_DISABLE_MODELS_FETCH"),
            opencode_disable_mouse: truthy("OPENCODE_DISABLE_MOUSE"),
            opencode_fake_vcs: std::env::var("OPENCODE_FAKE_VCS").ok(),
            opencode_server_password: std::env::var("OPENCODE_SERVER_PASSWORD").ok(),
            opencode_server_username: std::env::var("OPENCODE_SERVER_USERNAME").ok(),
            opencode_disable_fff: match fff {
                Some(v) => v == "1" || v.eq_ignore_ascii_case("true"),
                None => cfg!(target_os = "windows"),
            },
            opencode_experimental_filewatcher: false,
            opencode_experimental_disable_filewatcher: false,
            opencode_experimental_disable_copy_on_select: match copy {
                Some(v) => v == "1" || v.eq_ignore_ascii_case("true"),
                None => cfg!(target_os = "windows"),
            },
            opencode_models_url: std::env::var("OPENCODE_MODELS_URL").ok(),
            opencode_models_path: std::env::var("OPENCODE_MODELS_PATH").ok(),
            opencode_db: std::env::var("OPENCODE_DB").ok(),
            opencode_workspace_id: std::env::var("OPENCODE_WORKSPACE_ID").ok(),
            opencode_experimental_workspaces: enabled_by_experimental("OPENCODE_EXPERIMENTAL_WORKSPACES"),
            opencode_disable_project_config: truthy("OPENCODE_DISABLE_PROJECT_CONFIG"),
            opencode_experimental_references: enabled_by_experimental("OPENCODE_EXPERIMENTAL_REFERENCES"),
            opencode_tui_config: std::env::var("OPENCODE_TUI_CONFIG").ok(),
            opencode_config_dir: std::env::var("OPENCODE_CONFIG_DIR").ok(),
            opencode_pure: truthy("OPENCODE_PURE"),
            opencode_permission: std::env::var("OPENCODE_PERMISSION").ok(),
            opencode_plugin_meta_file: std::env::var("OPENCODE_PLUGIN_META_FILE").ok(),
            opencode_client: std::env::var("OPENCODE_CLIENT").unwrap_or_else(|_| "cli".to_string()),
        }
    }

    pub fn is_flag_set(&self, name: &str) -> bool {
        match name {
            "OPENCODE_AUTO_HEAP_SNAPSHOT" => self.opencode_auto_heap_snapshot,
            "OPENCODE_DISABLE_AUTOUPDATE" => self.opencode_disable_autoupdate,
            "OPENCODE_ALWAYS_NOTIFY_UPDATE" => self.opencode_always_notify_update,
            "OPENCODE_DISABLE_PRUNE" => self.opencode_disable_prune,
            "OPENCODE_DISABLE_TERMINAL_TITLE" => self.opencode_disable_terminal_title,
            "OPENCODE_SHOW_TTFD" => self.opencode_show_ttfd,
            "OPENCODE_DISABLE_AUTOCOMPACT" => self.opencode_disable_autocompact,
            "OPENCODE_DISABLE_MODELS_FETCH" => self.opencode_disable_models_fetch,
            "OPENCODE_DISABLE_MOUSE" => self.opencode_disable_mouse,
            "OPENCODE_DISABLE_FFF" => self.opencode_disable_fff,
            "OPENCODE_EXPERIMENTAL_FILEWATCHER" => self.opencode_experimental_filewatcher,
            "OPENCODE_EXPERIMENTAL_DISABLE_FILEWATCHER" => self.opencode_experimental_disable_filewatcher,
            "OPENCODE_EXPERIMENTAL_DISABLE_COPY_ON_SELECT" => self.opencode_experimental_disable_copy_on_select,
            "OPENCODE_EXPERIMENTAL_WORKSPACES" => self.opencode_experimental_workspaces,
            "OPENCODE_DISABLE_PROJECT_CONFIG" => self.opencode_disable_project_config,
            "OPENCODE_EXPERIMENTAL_REFERENCES" => self.opencode_experimental_references,
            "OPENCODE_PURE" => self.opencode_pure,
            _ => truthy(name),
        }
    }
}

impl Default for Flags {
    fn default() -> Self {
        Self::from_env()
    }
}

/// Check if a Rustcode feature flag is set from the environment.
/// "1" or "true" (case-insensitive) → true; everything else → false.
pub fn is_flag_set(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_flag_set_missing_env() {
        assert!(!is_flag_set("NONEXISTENT_FLAG_RUSTCODE_12345"));
    }

    #[test]
    fn test_is_flag_set_one() {
        std::env::set_var("RUSTCODE_TEST_FLAG_A", "1");
        assert!(is_flag_set("RUSTCODE_TEST_FLAG_A"));
        std::env::remove_var("RUSTCODE_TEST_FLAG_A");
    }

    #[test]
    fn test_is_flag_set_true_case_insensitive() {
        for val in &["true", "True", "TRUE", "tRuE"] {
            std::env::set_var("RUSTCODE_TEST_FLAG_B", val);
            assert!(is_flag_set("RUSTCODE_TEST_FLAG_B"));
        }
        std::env::remove_var("RUSTCODE_TEST_FLAG_B");
    }

    #[test]
    fn test_is_flag_set_false_values() {
        for val in &["0", "false", "no", "yes", "2", ""] {
            std::env::set_var("RUSTCODE_TEST_FLAG_C", val);
            assert!(!is_flag_set("RUSTCODE_TEST_FLAG_C"));
        }
        std::env::remove_var("RUSTCODE_TEST_FLAG_C");
    }

    #[test]
    fn test_git_bash_path_default_none() {}

    #[test]
    fn test_set_and_get_git_bash_path() {
        let path = std::env::var("OPENCODE_GIT_BASH_PATH")
            .ok()
            .filter(|s| !s.is_empty());
        assert!(path.is_none());
    }

    #[test]
    fn test_flags_from_env_defaults() {
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_client, "cli");
        assert!(!flags.opencode_disable_autoupdate);
        assert!(!flags.opencode_disable_prune);
    }

    #[test]
    fn test_flags_config_env_vars() {
        std::env::set_var("OPENCODE_CONFIG", "/tmp/test-config.json");
        std::env::set_var("OPENCODE_DISABLE_PROJECT_CONFIG", "1");
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_config.unwrap(), "/tmp/test-config.json");
        assert!(flags.opencode_disable_project_config);
        std::env::remove_var("OPENCODE_CONFIG");
        std::env::remove_var("OPENCODE_DISABLE_PROJECT_CONFIG");
    }

    #[test]
    fn test_flags_tui_config_var() {
        std::env::set_var("OPENCODE_TUI_CONFIG", "/tmp/tui.json");
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_tui_config.unwrap(), "/tmp/tui.json");
        std::env::remove_var("OPENCODE_TUI_CONFIG");
    }

    #[test]
    fn test_flags_client_default() {
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_client, "cli");
    }

    #[test]
    fn test_flags_client_custom() {
        std::env::set_var("OPENCODE_CLIENT", "desktop");
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_client, "desktop");
        std::env::remove_var("OPENCODE_CLIENT");
    }

    #[test]
    fn test_flags_permission_var() {
        std::env::set_var("OPENCODE_PERMISSION", r#"{"read":"deny"}"#);
        let flags = Flags::from_env();
        assert!(flags.opencode_permission.is_some());
        assert!(flags.opencode_permission.unwrap().contains("deny"));
        std::env::remove_var("OPENCODE_PERMISSION");
    }

    #[test]
    fn test_flags_server_credentials() {
        std::env::set_var("OPENCODE_SERVER_PASSWORD", "secret123");
        std::env::set_var("OPENCODE_SERVER_USERNAME", "admin");
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_server_password.unwrap(), "secret123");
        assert_eq!(flags.opencode_server_username.unwrap(), "admin");
        std::env::remove_var("OPENCODE_SERVER_PASSWORD");
        std::env::remove_var("OPENCODE_SERVER_USERNAME");
    }

    #[test]
    fn test_flags_models_vars() {
        std::env::set_var("OPENCODE_MODELS_URL", "https://models.example.com");
        std::env::set_var("OPENCODE_MODELS_PATH", "/tmp/models");
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_models_url.unwrap(), "https://models.example.com");
        assert_eq!(flags.opencode_models_path.unwrap(), "/tmp/models");
        std::env::remove_var("OPENCODE_MODELS_URL");
        std::env::remove_var("OPENCODE_MODELS_PATH");
    }

    #[test]
    fn test_flags_db_var() {
        std::env::set_var("OPENCODE_DB", "sqlite:///tmp/test.db");
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_db.unwrap(), "sqlite:///tmp/test.db");
        std::env::remove_var("OPENCODE_DB");
    }

    #[test]
    fn test_flags_workspace_id() {
        std::env::set_var("OPENCODE_WORKSPACE_ID", "ws_abc123");
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_workspace_id.unwrap(), "ws_abc123");
        std::env::remove_var("OPENCODE_WORKSPACE_ID");
    }

    #[test]
    fn test_flags_truthy_flags() {
        std::env::set_var("OPENCODE_DISABLE_MOUSE", "true");
        std::env::set_var("OPENCODE_SHOW_TTFD", "1");
        std::env::set_var("OPENCODE_DISABLE_TERMINAL_TITLE", "true");
        let flags = Flags::from_env();
        assert!(flags.opencode_disable_mouse);
        assert!(flags.opencode_show_ttfd);
        assert!(flags.opencode_disable_terminal_title);
        std::env::remove_var("OPENCODE_DISABLE_MOUSE");
        std::env::remove_var("OPENCODE_SHOW_TTFD");
        std::env::remove_var("OPENCODE_DISABLE_TERMINAL_TITLE");
    }

    #[test]
    fn test_flags_otlp_vars() {
        std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otel.example.com");
        std::env::set_var("OTEL_EXPORTER_OTLP_HEADERS", "x-api-key=abc");
        let flags = Flags::from_env();
        assert_eq!(flags.otel_exporter_otlp_endpoint.unwrap(), "https://otel.example.com");
        assert_eq!(flags.otel_exporter_otlp_headers.unwrap(), "x-api-key=abc");
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        std::env::remove_var("OTEL_EXPORTER_OTLP_HEADERS");
    }

    #[test]
    fn test_flags_fake_vcs() {
        std::env::set_var("OPENCODE_FAKE_VCS", "git");
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_fake_vcs.unwrap(), "git");
        std::env::remove_var("OPENCODE_FAKE_VCS");
    }

    #[test]
    fn test_flags_plugin_meta_file() {
        std::env::set_var("OPENCODE_PLUGIN_META_FILE", "/tmp/plugin-meta.json");
        let flags = Flags::from_env();
        assert_eq!(flags.opencode_plugin_meta_file.unwrap(), "/tmp/plugin-meta.json");
        std::env::remove_var("OPENCODE_PLUGIN_META_FILE");
    }

    #[test]
    fn test_flags_is_flag_set_lookup() {
        std::env::set_var("OPENCODE_PURE", "1");
        let flags = Flags::from_env();
        assert!(flags.is_flag_set("OPENCODE_PURE"));
        assert!(!flags.is_flag_set("OPENCODE_DISABLE_AUTOCOMPACT"));
        std::env::remove_var("OPENCODE_PURE");
    }

    #[test]
    fn test_flags_default_trait() {
        let flags = Flags::default();
        assert_eq!(flags.opencode_client, "cli");
    }

    #[test]
    fn test_flags_auto_heap_snapshot() {
        std::env::set_var("OPENCODE_AUTO_HEAP_SNAPSHOT", "true");
        let flags = Flags::from_env();
        assert!(flags.opencode_auto_heap_snapshot);
        std::env::remove_var("OPENCODE_AUTO_HEAP_SNAPSHOT");
    }

    #[test]
    fn test_flags_notify_update() {
        std::env::set_var("OPENCODE_ALWAYS_NOTIFY_UPDATE", "1");
        let flags = Flags::from_env();
        assert!(flags.opencode_always_notify_update);
        std::env::remove_var("OPENCODE_ALWAYS_NOTIFY_UPDATE");
    }

    #[test]
    fn test_enabled_by_experimental() {
        std::env::set_var("OPENCODE_EXPERIMENTAL", "true");
        assert!(enabled_by_experimental("OPENCODE_EXPERIMENTAL_WORKSPACES"));
        std::env::remove_var("OPENCODE_EXPERIMENTAL");
    }

    #[test]
    fn test_truthy_edge_cases() {
        assert!(!truthy("NOT_SET_VAR_XYZ_12345"));
    }
}
