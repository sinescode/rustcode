//! Installation management — version detection, upgrade methods, and user-agent.
//!
//! Ported from: `packages/blazecode/src/installation/index.ts`
//! BlazeCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! Provides:
//! - [`Method`] — detected installation method (curl, npm, brew, etc.)
//! - [`ReleaseType`] — semver release classification (patch, minor, major)
//! - [`InstallationInfo`] — current + latest version info
//! - [`user_agent`] — build the user-agent string for HTTP requests

use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────

/// Installation method detected from the runtime environment.
///
/// # Source
/// `packages/blazecode/src/installation/index.ts` line 17.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Curl,
    Npm,
    Yarn,
    Pnpm,
    Bun,
    Brew,
    Scoop,
    Choco,
    Unknown,
}

/// Semver release type classification.
///
/// # Source
/// `packages/blazecode/src/installation/index.ts` line 19.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseType {
    Patch,
    Minor,
    Major,
}

/// Installation version information.
///
/// # Source
/// `packages/blazecode/src/installation/index.ts` lines 47–51 (`Info`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationInfo {
    pub version: String,
    pub latest: String,
}

// ── Release type detection ────────────────────────────────────────────

/// Classify the difference between two semver versions as patch, minor, or major.
///
/// # Source
/// `packages/blazecode/src/installation/index.ts` lines 36–45.
pub fn get_release_type(current: &str, latest: &str) -> ReleaseType {
    let curr_parts = parse_semver(current);
    let latest_parts = parse_semver(latest);

    if latest_parts.0 > curr_parts.0 {
        ReleaseType::Major
    } else if latest_parts.1 > curr_parts.1 {
        ReleaseType::Minor
    } else {
        ReleaseType::Patch
    }
}

/// Parse a `major.minor.patch` string into a tuple of `(u64, u64, u64)`.
/// Non-numeric segments default to 0.
fn parse_semver(v: &str) -> (u64, u64, u64) {
    let parts: Vec<&str> = v.split('.').collect();
    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

// ── User-agent ────────────────────────────────────────────────────────

/// Build the user-agent string for HTTP requests.
///
/// Format: `blazecode/{channel}/{version}/{client}`
///
/// # Source
/// `packages/blazecode/src/installation/index.ts` lines 53–55.
pub fn user_agent(client: &str, channel: &str, version: &str) -> String {
    format!("blazecode/{channel}/{version}/{client}")
}

// ── InstallationService ─────────────────────────────────────────────────

/// Service for detecting installation method and performing upgrades.
///
/// Ported from: `packages/blazecode/src/installation/index.ts` lines 87–348.
pub struct InstallationService {
    http_client: reqwest::Client,
}

impl InstallationService {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
        }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { http_client: client }
    }

    /// Detect the installation method by examining the running binary path
    /// and running package-manager-specific commands.
    ///
    /// Ported from: `packages/blazecode/src/installation/index.ts` lines 186–219.
    pub fn method(&self) -> Method {
        let exec = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if exec.contains(".blazecode") && exec.contains("bin") {
            return Method::Curl;
        }
        if exec.contains(".local") && exec.contains("bin") {
            return Method::Curl;
        }

        let checks: Vec<(Method, &str, &str)> = vec![
            (Method::Npm, "npm", "blazecode-ai"),
            (Method::Bun, "bun", "blazecode-ai"),
            (Method::Pnpm, "pnpm", "blazecode-ai"),
            (Method::Yarn, "yarn", "blazecode-ai"),
            (Method::Brew, "brew", "blazecode"),
            (Method::Scoop, "scoop", "blazecode"),
            (Method::Choco, "choco", "blazecode"),
        ];

        for (method, cmd, check_name) in &checks {
            if exec.contains(cmd) {
                if let Ok(output) = std::process::Command::new(cmd)
                    .args(method_check_args(cmd))
                    .output()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if stdout.contains(check_name) {
                        return method.clone();
                    }
                }
            }
        }

        Method::Unknown
    }

    /// Fetch the latest available version string.
    ///
    /// Ported from: `packages/blazecode/src/installation/index.ts` lines 220–276.
    pub fn latest(&self, method: Option<Method>) -> Result<String, String> {
        let m = method.unwrap_or_else(|| self.method());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;

        rt.block_on(async {
            match m {
                Method::Brew => {
                    let formula = self.get_brew_formula().await;
                    if formula.contains('/') {
                        let info_json = run_cmd_str(&["brew", "info", "--json=v2", &formula]).await;
                        if let Ok(v) = parse_brew_info_v2(&info_json) {
                            return Ok(v);
                        }
                    }
                    let resp = self
                        .http_client
                        .get("https://formulae.brew.sh/api/formula/blazecode.json")
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                    data["versions"]["stable"]
                        .as_str()
                        .map(String::from)
                        .ok_or_else(|| "no stable version in brew formula".to_string())
                }
                Method::Npm | Method::Bun | Method::Pnpm => {
                    let registry = std::env::var("NPM_CONFIG_REGISTRY")
                        .unwrap_or_else(|_| "https://registry.npmjs.org".to_string());
                    let url = format!("{}/blazecode-ai/latest", registry.trim_end_matches('/'));
                    let resp = self
                        .http_client
                        .get(&url)
                        .header("Accept", "application/json")
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                    data["version"]
                        .as_str()
                        .map(String::from)
                        .ok_or_else(|| "no version in npm response".to_string())
                }
                Method::Choco => {
                    let url = "https://community.chocolatey.org/api/v2/Packages?$filter=Id%20eq%20%27blazecode%27%20and%20IsLatestVersion&$select=Version";
                    let resp = self
                        .http_client
                        .get(url)
                        .header("Accept", "application/json;odata=verbose")
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                    data["d"]["results"][0]["Version"]
                        .as_str()
                        .map(String::from)
                        .ok_or_else(|| "no version in choco response".to_string())
                }
                Method::Scoop => {
                    let url = "https://raw.githubusercontent.com/ScoopInstaller/Master/master/bucket/blazecode.json";
                    let resp = self
                        .http_client
                        .get(url)
                        .header("Accept", "application/json")
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                    data["version"]
                        .as_str()
                        .map(String::from)
                        .ok_or_else(|| "no version in scoop manifest".to_string())
                }
                _ => {
                    let resp = self
                        .http_client
                        .get("https://api.github.com/repos/anomalyco/blazecode/releases/latest")
                        .header("Accept", "application/json")
                        .header("User-Agent", "blazecode")
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                    let tag = data["tag_name"]
                        .as_str()
                        .ok_or_else(|| "no tag_name in github response".to_string())?;
                    Ok(tag.strip_prefix('v').unwrap_or(tag).to_string())
                }
            }
        })
    }

    /// Upgrade to `target` version using the given installation method.
    ///
    /// Ported from: `packages/blazecode/src/installation/index.ts` lines 277–333.
    pub fn upgrade(&self, method: &Method, target: &str) -> Result<(), String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;

        rt.block_on(async {
            match method {
                Method::Curl => {
                    let install_script = self
                        .http_client
                        .get("https://blazecode.ai/install")
                        .send()
                        .await
                        .map_err(|e| format!("failed to fetch install script: {e}"))?
                        .text()
                        .await
                        .map_err(|e| format!("failed to read install script: {e}"))?;
                    let shell = if which("bash").is_some() { "bash" } else { "sh" };
                    let mut child = tokio::process::Command::new(shell)
                        .env("VERSION", target)
                        .stdin(std::process::Stdio::piped())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .spawn()
                        .map_err(|e| format!("failed to spawn {shell}: {e}"))?;
                    use tokio::io::AsyncWriteExt;
                    if let Some(mut stdin) = child.stdin.take() {
                        stdin.write_all(install_script.as_bytes()).await.ok();
                    }
                    let output = child.wait_with_output().await.map_err(|e| e.to_string())?;
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        return Err(format!("curl upgrade failed: {stderr}"));
                    }
                    Ok(())
                }
                Method::Npm => run_cmd(&["npm", "install", "-g", &format!("blazecode-ai@{target}")]).await,
                Method::Pnpm => run_cmd(&["pnpm", "install", "-g", &format!("blazecode-ai@{target}")]).await,
                Method::Bun => run_cmd(&["bun", "install", "-g", &format!("blazecode-ai@{target}")]).await,
                Method::Yarn => run_cmd(&["yarn", "global", "add", &format!("blazecode-ai@{target}")]).await,
                Method::Brew => {
                    let formula = self.get_brew_formula().await;
                    let _ = run_cmd(&["brew", "tap", "anomalyco/tap"]).await;
                    run_cmd(&["brew", "upgrade", &formula]).await
                }
                Method::Choco => {
                    run_cmd(&["choco", "upgrade", "blazecode", &format!("--version={target}"), "-y"]).await
                }
                Method::Scoop => {
                    run_cmd(&["scoop", "install", &format!("blazecode@{target}")]).await
                }
                Method::Unknown => Err("Unknown installation method".to_string()),
            }
        })
    }

    async fn get_brew_formula(&self) -> String {
        let tap = run_cmd_str(&["brew", "list", "--formula", "anomalyco/tap/blazecode"]).await;
        if tap.contains("blazecode") {
            return "anomalyco/tap/blazecode".to_string();
        }
        let core = run_cmd_str(&["brew", "list", "--formula", "blazecode"]).await;
        if core.contains("blazecode") {
            return "blazecode".to_string();
        }
        "blazecode".to_string()
    }
}

impl Default for InstallationService {
    fn default() -> Self {
        Self::new()
    }
}

fn method_check_args(cmd: &str) -> &[&str] {
    match cmd {
        "npm" => &["list", "-g", "--depth=0"],
        "yarn" => &["global", "list"],
        "pnpm" => &["list", "-g", "--depth=0"],
        "bun" => &["pm", "ls", "-g"],
        "brew" => &["list", "--formula", "blazecode"],
        "scoop" => &["list", "blazecode"],
        "choco" => &["list", "--limit-output", "blazecode"],
        _ => &[],
    }
}

async fn run_cmd(args: &[&str]) -> Result<(), String> {
    let output = tokio::process::Command::new(args[0])
        .args(&args[1..])
        .output()
        .await
        .map_err(|e| format!("failed to spawn {}: {e}", args[0]))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("{} failed: {stderr}", args[0]))
    }
}

async fn run_cmd_str(args: &[&str]) -> String {
    tokio::process::Command::new(args[0])
        .args(&args[1..])
        .output()
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn which(name: &str) -> Option<String> {
    std::env::var_os("PATH").and_then(|path_var| {
        std::env::split_paths(&path_var).find_map(|dir| {
            let path = dir.join(name);
            if path.is_file() {
                Some(path.to_string_lossy().to_string())
            } else {
                None
            }
        })
    })
}

fn parse_brew_info_v2(json: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    v["formulae"][0]["versions"]["stable"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "no stable version".to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_release_type_patch() {
        assert_eq!(get_release_type("1.2.3", "1.2.4"), ReleaseType::Patch);
    }

    #[test]
    fn test_get_release_type_minor() {
        assert_eq!(get_release_type("1.2.3", "1.3.0"), ReleaseType::Minor);
    }

    #[test]
    fn test_get_release_type_major() {
        assert_eq!(get_release_type("1.2.3", "2.0.0"), ReleaseType::Major);
    }

    #[test]
    fn test_get_release_type_same() {
        assert_eq!(get_release_type("1.2.3", "1.2.3"), ReleaseType::Patch);
    }

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("1.2.3"), (1, 2, 3));
        assert_eq!(parse_semver("0.0.1"), (0, 0, 1));
        assert_eq!(parse_semver("10.20.30"), (10, 20, 30));
        assert_eq!(parse_semver("1.2"), (1, 2, 0));
        assert_eq!(parse_semver("1"), (1, 0, 0));
    }

    #[test]
    fn test_user_agent() {
        let ua = user_agent("cli", "latest", "1.0.0");
        assert_eq!(ua, "blazecode/latest/1.0.0/cli");
    }

    // ── InstallationService tests ─────────────────────────────────────

    #[test]
    fn test_installation_service_construct() {
        let svc = InstallationService::new();
        // Should not panic
    }

    #[test]
    fn test_installation_service_with_client() {
        let client = reqwest::Client::new();
        let svc = InstallationService::with_client(client);
    }

    #[test]
    fn test_installation_service_default() {
        let svc = InstallationService::default();
    }

    #[test]
    fn test_parse_brew_info_v2_valid() {
        let json = r#"{"formulae": [{"versions": {"stable": "1.2.3"}}]}"#;
        let version = parse_brew_info_v2(json).unwrap();
        assert_eq!(version, "1.2.3");
    }

    #[test]
    fn test_parse_brew_info_v2_missing() {
        let json = r#"{"formulae": []}"#;
        let result = parse_brew_info_v2(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_brew_info_v2_invalid() {
        let result = parse_brew_info_v2("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_method_check_args() {
        let args = method_check_args("npm");
        assert!(args.contains(&"-g"));

        let args = method_check_args("brew");
        assert!(args.contains(&"blazecode"));

        let args = method_check_args("unknown");
        assert!(args.is_empty());
    }

    #[test]
    fn test_installation_info_serialize() {
        let info = InstallationInfo {
            version: "1.0.0".into(),
            latest: "1.2.3".into(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("1.0.0"));
        assert!(json.contains("1.2.3"));
    }

    #[test]
    fn test_installation_info_deserialize() {
        let json = r#"{"version": "2.0.0", "latest": "2.0.1"}"#;
        let info: InstallationInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.version, "2.0.0");
        assert_eq!(info.latest, "2.0.1");
    }

    #[test]
    fn test_method_enum_serialize() {
        let m = Method::Curl;
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, "\"curl\"");

        let m = Method::Npm;
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, "\"npm\"");
    }

    #[test]
    fn test_method_enum_deserialize() {
        let m: Method = serde_json::from_str("\"bun\"").unwrap();
        assert_eq!(m, Method::Bun);

        let m: Method = serde_json::from_str("\"unknown\"").unwrap();
        assert_eq!(m, Method::Unknown);
    }

    #[test]
    fn test_release_type_enum_serialize() {
        let r = ReleaseType::Major;
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, "\"major\"");
    }
}
