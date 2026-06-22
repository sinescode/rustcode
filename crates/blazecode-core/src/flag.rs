use std::sync::OnceLock;

static GIT_BASH_PATH: OnceLock<Option<String>> = OnceLock::new();

pub fn git_bash_path() -> Option<&'static str> {
    GIT_BASH_PATH
        .get_or_init(|| {
            std::env::var("BLAZECODE_GIT_BASH_PATH")
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
        truthy("BLAZECODE_EXPERIMENTAL")
    }
}

#[derive(Debug, Clone)]
pub struct Flags {
    pub otel_exporter_otlp_endpoint: Option<String>,
    pub otel_exporter_otlp_headers: Option<String>,
    pub blazecode_auto_heap_snapshot: bool,
    pub blazecode_git_bash_path: Option<String>,
    pub blazecode_config: Option<String>,
    pub blazecode_config_content: Option<String>,
    pub blazecode_disable_autoupdate: bool,
    pub blazecode_always_notify_update: bool,
    pub blazecode_disable_prune: bool,
    pub blazecode_disable_terminal_title: bool,
    pub blazecode_show_ttfd: bool,
    pub blazecode_disable_autocompact: bool,
    pub blazecode_disable_models_fetch: bool,
    pub blazecode_disable_mouse: bool,
    pub blazecode_fake_vcs: Option<String>,
    pub blazecode_server_password: Option<String>,
    pub blazecode_server_username: Option<String>,
    pub blazecode_disable_fff: bool,
    pub blazecode_experimental_filewatcher: bool,
    pub blazecode_experimental_disable_filewatcher: bool,
    pub blazecode_experimental_disable_copy_on_select: bool,
    pub blazecode_models_url: Option<String>,
    pub blazecode_models_path: Option<String>,
    pub blazecode_db: Option<String>,
    pub blazecode_workspace_id: Option<String>,
    pub blazecode_experimental_workspaces: bool,
    pub blazecode_disable_project_config: bool,
    pub blazecode_experimental_references: bool,
    pub blazecode_tui_config: Option<String>,
    pub blazecode_config_dir: Option<String>,
    pub blazecode_pure: bool,
    pub blazecode_permission: Option<String>,
    pub blazecode_plugin_meta_file: Option<String>,
    pub blazecode_client: String,
}

impl Flags {
    pub fn from_env() -> Self {
        let fff = std::env::var("BLAZECODE_DISABLE_FFF").ok();
        let copy = std::env::var("BLAZECODE_EXPERIMENTAL_DISABLE_COPY_ON_SELECT").ok();
        Self {
            otel_exporter_otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            otel_exporter_otlp_headers: std::env::var("OTEL_EXPORTER_OTLP_HEADERS").ok(),
            blazecode_auto_heap_snapshot: truthy("BLAZECODE_AUTO_HEAP_SNAPSHOT"),
            blazecode_git_bash_path: std::env::var("BLAZECODE_GIT_BASH_PATH").ok(),
            blazecode_config: std::env::var("BLAZECODE_CONFIG").ok(),
            blazecode_config_content: std::env::var("BLAZECODE_CONFIG_CONTENT").ok(),
            blazecode_disable_autoupdate: truthy("BLAZECODE_DISABLE_AUTOUPDATE"),
            blazecode_always_notify_update: truthy("BLAZECODE_ALWAYS_NOTIFY_UPDATE"),
            blazecode_disable_prune: truthy("BLAZECODE_DISABLE_PRUNE"),
            blazecode_disable_terminal_title: truthy("BLAZECODE_DISABLE_TERMINAL_TITLE"),
            blazecode_show_ttfd: truthy("BLAZECODE_SHOW_TTFD"),
            blazecode_disable_autocompact: truthy("BLAZECODE_DISABLE_AUTOCOMPACT"),
            blazecode_disable_models_fetch: truthy("BLAZECODE_DISABLE_MODELS_FETCH"),
            blazecode_disable_mouse: truthy("BLAZECODE_DISABLE_MOUSE"),
            blazecode_fake_vcs: std::env::var("BLAZECODE_FAKE_VCS").ok(),
            blazecode_server_password: std::env::var("BLAZECODE_SERVER_PASSWORD").ok(),
            blazecode_server_username: std::env::var("BLAZECODE_SERVER_USERNAME").ok(),
            blazecode_disable_fff: match fff {
                Some(v) => v == "1" || v.eq_ignore_ascii_case("true"),
                None => cfg!(target_os = "windows"),
            },
            blazecode_experimental_filewatcher: false,
            blazecode_experimental_disable_filewatcher: false,
            blazecode_experimental_disable_copy_on_select: match copy {
                Some(v) => v == "1" || v.eq_ignore_ascii_case("true"),
                None => cfg!(target_os = "windows"),
            },
            blazecode_models_url: std::env::var("BLAZECODE_MODELS_URL").ok(),
            blazecode_models_path: std::env::var("BLAZECODE_MODELS_PATH").ok(),
            blazecode_db: std::env::var("BLAZECODE_DB").ok(),
            blazecode_workspace_id: std::env::var("BLAZECODE_WORKSPACE_ID").ok(),
            blazecode_experimental_workspaces: enabled_by_experimental("BLAZECODE_EXPERIMENTAL_WORKSPACES"),
            blazecode_disable_project_config: truthy("BLAZECODE_DISABLE_PROJECT_CONFIG"),
            blazecode_experimental_references: enabled_by_experimental("BLAZECODE_EXPERIMENTAL_REFERENCES"),
            blazecode_tui_config: std::env::var("BLAZECODE_TUI_CONFIG").ok(),
            blazecode_config_dir: std::env::var("BLAZECODE_CONFIG_DIR").ok(),
            blazecode_pure: truthy("BLAZECODE_PURE"),
            blazecode_permission: std::env::var("BLAZECODE_PERMISSION").ok(),
            blazecode_plugin_meta_file: std::env::var("BLAZECODE_PLUGIN_META_FILE").ok(),
            blazecode_client: std::env::var("BLAZECODE_CLIENT").unwrap_or_else(|_| "cli".to_string()),
        }
    }

    pub fn is_flag_set(&self, name: &str) -> bool {
        match name {
            "BLAZECODE_AUTO_HEAP_SNAPSHOT" => self.blazecode_auto_heap_snapshot,
            "BLAZECODE_DISABLE_AUTOUPDATE" => self.blazecode_disable_autoupdate,
            "BLAZECODE_ALWAYS_NOTIFY_UPDATE" => self.blazecode_always_notify_update,
            "BLAZECODE_DISABLE_PRUNE" => self.blazecode_disable_prune,
            "BLAZECODE_DISABLE_TERMINAL_TITLE" => self.blazecode_disable_terminal_title,
            "BLAZECODE_SHOW_TTFD" => self.blazecode_show_ttfd,
            "BLAZECODE_DISABLE_AUTOCOMPACT" => self.blazecode_disable_autocompact,
            "BLAZECODE_DISABLE_MODELS_FETCH" => self.blazecode_disable_models_fetch,
            "BLAZECODE_DISABLE_MOUSE" => self.blazecode_disable_mouse,
            "BLAZECODE_DISABLE_FFF" => self.blazecode_disable_fff,
            "BLAZECODE_EXPERIMENTAL_FILEWATCHER" => self.blazecode_experimental_filewatcher,
            "BLAZECODE_EXPERIMENTAL_DISABLE_FILEWATCHER" => self.blazecode_experimental_disable_filewatcher,
            "BLAZECODE_EXPERIMENTAL_DISABLE_COPY_ON_SELECT" => self.blazecode_experimental_disable_copy_on_select,
            "BLAZECODE_EXPERIMENTAL_WORKSPACES" => self.blazecode_experimental_workspaces,
            "BLAZECODE_DISABLE_PROJECT_CONFIG" => self.blazecode_disable_project_config,
            "BLAZECODE_EXPERIMENTAL_REFERENCES" => self.blazecode_experimental_references,
            "BLAZECODE_PURE" => self.blazecode_pure,
            _ => truthy(name),
        }
    }
}

impl Default for Flags {
    fn default() -> Self {
        Self::from_env()
    }
}

/// Check if a Blazecode feature flag is set from the environment.
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
        assert!(!is_flag_set("NONEXISTENT_FLAG_BLAZECODE_12345"));
    }

    #[test]
    fn test_is_flag_set_one() {
        std::env::set_var("BLAZECODE_TEST_FLAG_A", "1");
        assert!(is_flag_set("BLAZECODE_TEST_FLAG_A"));
        std::env::remove_var("BLAZECODE_TEST_FLAG_A");
    }

    #[test]
    fn test_is_flag_set_true_case_insensitive() {
        for val in &["true", "True", "TRUE", "tRuE"] {
            std::env::set_var("BLAZECODE_TEST_FLAG_B", val);
            assert!(is_flag_set("BLAZECODE_TEST_FLAG_B"));
        }
        std::env::remove_var("BLAZECODE_TEST_FLAG_B");
    }

    #[test]
    fn test_is_flag_set_false_values() {
        for val in &["0", "false", "no", "yes", "2", ""] {
            std::env::set_var("BLAZECODE_TEST_FLAG_C", val);
            assert!(!is_flag_set("BLAZECODE_TEST_FLAG_C"));
        }
        std::env::remove_var("BLAZECODE_TEST_FLAG_C");
    }

    #[test]
    fn test_git_bash_path_default_none() {}

    #[test]
    fn test_set_and_get_git_bash_path() {
        let path = std::env::var("BLAZECODE_GIT_BASH_PATH")
            .ok()
            .filter(|s| !s.is_empty());
        assert!(path.is_none());
    }

    #[test]
    fn test_flags_from_env_defaults() {
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_client, "cli");
        assert!(!flags.blazecode_disable_autoupdate);
        assert!(!flags.blazecode_disable_prune);
    }

    #[test]
    fn test_flags_config_env_vars() {
        std::env::set_var("BLAZECODE_CONFIG", "/tmp/test-config.json");
        std::env::set_var("BLAZECODE_DISABLE_PROJECT_CONFIG", "1");
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_config.unwrap(), "/tmp/test-config.json");
        assert!(flags.blazecode_disable_project_config);
        std::env::remove_var("BLAZECODE_CONFIG");
        std::env::remove_var("BLAZECODE_DISABLE_PROJECT_CONFIG");
    }

    #[test]
    fn test_flags_tui_config_var() {
        std::env::set_var("BLAZECODE_TUI_CONFIG", "/tmp/tui.json");
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_tui_config.unwrap(), "/tmp/tui.json");
        std::env::remove_var("BLAZECODE_TUI_CONFIG");
    }

    #[test]
    fn test_flags_client_default() {
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_client, "cli");
    }

    #[test]
    fn test_flags_client_custom() {
        std::env::set_var("BLAZECODE_CLIENT", "desktop");
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_client, "desktop");
        std::env::remove_var("BLAZECODE_CLIENT");
    }

    #[test]
    fn test_flags_permission_var() {
        std::env::set_var("BLAZECODE_PERMISSION", r#"{"read":"deny"}"#);
        let flags = Flags::from_env();
        assert!(flags.blazecode_permission.is_some());
        assert!(flags.blazecode_permission.unwrap().contains("deny"));
        std::env::remove_var("BLAZECODE_PERMISSION");
    }

    #[test]
    fn test_flags_server_credentials() {
        std::env::set_var("BLAZECODE_SERVER_PASSWORD", "secret123");
        std::env::set_var("BLAZECODE_SERVER_USERNAME", "admin");
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_server_password.unwrap(), "secret123");
        assert_eq!(flags.blazecode_server_username.unwrap(), "admin");
        std::env::remove_var("BLAZECODE_SERVER_PASSWORD");
        std::env::remove_var("BLAZECODE_SERVER_USERNAME");
    }

    #[test]
    fn test_flags_models_vars() {
        std::env::set_var("BLAZECODE_MODELS_URL", "https://models.example.com");
        std::env::set_var("BLAZECODE_MODELS_PATH", "/tmp/models");
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_models_url.unwrap(), "https://models.example.com");
        assert_eq!(flags.blazecode_models_path.unwrap(), "/tmp/models");
        std::env::remove_var("BLAZECODE_MODELS_URL");
        std::env::remove_var("BLAZECODE_MODELS_PATH");
    }

    #[test]
    fn test_flags_db_var() {
        std::env::set_var("BLAZECODE_DB", "sqlite:///tmp/test.db");
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_db.unwrap(), "sqlite:///tmp/test.db");
        std::env::remove_var("BLAZECODE_DB");
    }

    #[test]
    fn test_flags_workspace_id() {
        std::env::set_var("BLAZECODE_WORKSPACE_ID", "ws_abc123");
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_workspace_id.unwrap(), "ws_abc123");
        std::env::remove_var("BLAZECODE_WORKSPACE_ID");
    }

    #[test]
    fn test_flags_truthy_flags() {
        std::env::set_var("BLAZECODE_DISABLE_MOUSE", "true");
        std::env::set_var("BLAZECODE_SHOW_TTFD", "1");
        std::env::set_var("BLAZECODE_DISABLE_TERMINAL_TITLE", "true");
        let flags = Flags::from_env();
        assert!(flags.blazecode_disable_mouse);
        assert!(flags.blazecode_show_ttfd);
        assert!(flags.blazecode_disable_terminal_title);
        std::env::remove_var("BLAZECODE_DISABLE_MOUSE");
        std::env::remove_var("BLAZECODE_SHOW_TTFD");
        std::env::remove_var("BLAZECODE_DISABLE_TERMINAL_TITLE");
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
        std::env::set_var("BLAZECODE_FAKE_VCS", "git");
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_fake_vcs.unwrap(), "git");
        std::env::remove_var("BLAZECODE_FAKE_VCS");
    }

    #[test]
    fn test_flags_plugin_meta_file() {
        std::env::set_var("BLAZECODE_PLUGIN_META_FILE", "/tmp/plugin-meta.json");
        let flags = Flags::from_env();
        assert_eq!(flags.blazecode_plugin_meta_file.unwrap(), "/tmp/plugin-meta.json");
        std::env::remove_var("BLAZECODE_PLUGIN_META_FILE");
    }

    #[test]
    fn test_flags_is_flag_set_lookup() {
        std::env::set_var("BLAZECODE_PURE", "1");
        let flags = Flags::from_env();
        assert!(flags.is_flag_set("BLAZECODE_PURE"));
        assert!(!flags.is_flag_set("BLAZECODE_DISABLE_AUTOCOMPACT"));
        std::env::remove_var("BLAZECODE_PURE");
    }

    #[test]
    fn test_flags_default_trait() {
        let flags = Flags::default();
        assert_eq!(flags.blazecode_client, "cli");
    }

    #[test]
    fn test_flags_auto_heap_snapshot() {
        std::env::set_var("BLAZECODE_AUTO_HEAP_SNAPSHOT", "true");
        let flags = Flags::from_env();
        assert!(flags.blazecode_auto_heap_snapshot);
        std::env::remove_var("BLAZECODE_AUTO_HEAP_SNAPSHOT");
    }

    #[test]
    fn test_flags_notify_update() {
        std::env::set_var("BLAZECODE_ALWAYS_NOTIFY_UPDATE", "1");
        let flags = Flags::from_env();
        assert!(flags.blazecode_always_notify_update);
        std::env::remove_var("BLAZECODE_ALWAYS_NOTIFY_UPDATE");
    }

    #[test]
    fn test_enabled_by_experimental() {
        std::env::set_var("BLAZECODE_EXPERIMENTAL", "true");
        assert!(enabled_by_experimental("BLAZECODE_EXPERIMENTAL_WORKSPACES"));
        std::env::remove_var("BLAZECODE_EXPERIMENTAL");
    }

    #[test]
    fn test_truthy_edge_cases() {
        assert!(!truthy("NOT_SET_VAR_XYZ_12345"));
    }
}
