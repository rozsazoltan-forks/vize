//! LSP Client for tsgo.
//!
//! Communicates with tsgo LSP server to perform type checking on virtual files
//! without writing them to disk.
//!
//! ## Submodules
//!
//! - `requests`: JSON-RPC request/response protocol and document management
//! - `handlers`: Notification handling, message draining, and diagnostics collection

mod handlers;
mod requests;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::BufReader,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicI64, AtomicUsize, Ordering},
    thread,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use vize_carton::cstr;
use vize_carton::FxHashMap;
use vize_carton::String;
use vize_carton::ToCompactString;

/// LSP Client for tsgo
pub struct TsgoLspClient {
    process: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: BufReader<ChildStdout>,
    pub(crate) request_id: AtomicI64,
    /// Pending diagnostics received via publishDiagnostics
    pub(crate) diagnostics: FxHashMap<String, Vec<LspDiagnostic>>,
    /// Temporary directory for tsconfig.json (cleaned up on drop)
    temp_dir: Option<std::path::PathBuf>,
}

/// LSP Diagnostic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDiagnostic {
    pub range: LspRange,
    pub severity: Option<i32>,
    pub code: Option<Value>,
    pub source: Option<String>,
    pub message: String,
}

/// LSP Range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

/// LSP Position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

fn find_node_modules_with_vue(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        let node_modules = dir.join("node_modules");
        if node_modules.join("vue").is_dir() {
            return Some(node_modules);
        }
        dir = dir.parent()?;
    }
}

fn resolve_temp_dir_base(project_root: Option<&Path>) -> PathBuf {
    let fallback_root = project_root
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    find_node_modules_with_vue(&fallback_root)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or(fallback_root)
        .join("__agent_only")
        .join("vize-tsgo")
}

impl TsgoLspClient {
    /// Start tsgo LSP server
    ///
    /// tsgo path resolution order:
    /// 1. Explicit tsgo_path argument
    /// 2. TSGO_PATH environment variable
    /// 3. Local node_modules (relative to working_dir or cwd)
    /// 4. Common npm global install locations
    /// 5. "tsgo" in PATH
    pub fn new(tsgo_path: Option<&str>, working_dir: Option<&str>) -> Result<Self, String> {
        let tsgo: String = tsgo_path
            .map(String::from)
            .or_else(|| std::env::var("TSGO_PATH").ok().map(String::from))
            .or_else(|| Self::find_tsgo_in_local_node_modules(working_dir))
            .or_else(Self::find_tsgo_in_common_locations)
            .unwrap_or_else(|| "tsgo".into());

        eprintln!("\x1b[90m[tsgo] Using: {tsgo}\x1b[0m");

        // Determine project root (for node_modules resolution)
        let project_root = working_dir
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .and_then(|p| p.canonicalize().ok());

        // Create an isolated agent-owned directory with a proper tsconfig.json.
        // This ensures tsgo uses ES module mode (import.meta, dynamic import, etc.)
        // regardless of the project's tsconfig.json state.
        static NEXT_CLIENT_ID: AtomicUsize = AtomicUsize::new(0);

        let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
        let temp_dir_base = resolve_temp_dir_base(project_root.as_deref());
        let temp_dir_path = temp_dir_base.join(&*cstr!("{}-{}", std::process::id(), client_id));

        let _ = std::fs::remove_dir_all(&temp_dir_path);
        std::fs::create_dir_all(&temp_dir_path)
            .map_err(|e| cstr!("Failed to create agent directory: {e}"))?;

        // Find and symlink node_modules so tsgo can resolve packages (e.g., 'vue').
        // Walk up from project root to find node_modules that contains 'vue'.
        let node_modules_path = project_root.as_deref().and_then(find_node_modules_with_vue);
        if let Some(ref nm_path) = node_modules_path {
            let symlink_target = temp_dir_path.join("node_modules");
            // Remove stale symlink if exists
            let _ = std::fs::remove_file(&symlink_target);
            #[cfg(unix)]
            {
                let _ = std::os::unix::fs::symlink(nm_path, &symlink_target);
            }
            #[cfg(windows)]
            {
                let _ = std::os::windows::fs::symlink_dir(nm_path, &symlink_target);
            }
        }

        let tsconfig_content = serde_json::json!({
            "compilerOptions": {
                "target": "ES2022",
                "module": "ESNext",
                "moduleResolution": "bundler",
                "lib": ["ES2022", "DOM", "DOM.Iterable"],
                "strict": true,
                "noEmit": true,
                "skipLibCheck": true
            }
        });
        std::fs::write(
            temp_dir_path.join("tsconfig.json"),
            tsconfig_content.to_compact_string(),
        )
        .map_err(|e| cstr!("Failed to write temp tsconfig.json: {e}"))?;

        let mut cmd = Command::new(tsgo.as_str());
        cmd.arg("--lsp")
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Use temp directory as working directory so tsgo picks up our tsconfig.json
        cmd.current_dir(&temp_dir_path);

        let mut process = cmd
            .spawn()
            .map_err(|e| cstr!("Failed to start tsgo lsp: {e}"))?;

        let stdin = process
            .stdin
            .take()
            .ok_or("Failed to get stdin of tsgo lsp")?;
        let stdout = process
            .stdout
            .take()
            .ok_or("Failed to get stdout of tsgo lsp")?;

        // Set stdout to non-blocking mode on Unix
        #[cfg(unix)]
        {
            use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};
            let fd = stdout.as_raw_fd();
            unsafe {
                let flags = fcntl(fd, F_GETFL);
                fcntl(fd, F_SETFL, flags | O_NONBLOCK);
            }
        }

        // Use temp dir path as rootUri for LSP initialization
        let temp_root = temp_dir_path.canonicalize().ok();
        let mut client = Self {
            process,
            stdin,
            stdout: BufReader::new(stdout),
            request_id: AtomicI64::new(1),
            diagnostics: FxHashMap::default(),
            temp_dir: Some(temp_dir_path),
        };

        // Initialize LSP with temp directory for tsconfig resolution
        client.initialize(temp_root.as_ref())?;

        Ok(client)
    }

    /// Initialize LSP connection
    fn initialize(&mut self, project_root: Option<&std::path::PathBuf>) -> Result<(), String> {
        // Convert project root to file:// URI
        let root_uri = project_root.map(|p| cstr!("file://{}", p.display()));

        let workspace_folders = root_uri.as_ref().map(|uri| {
            serde_json::json!([{
                "uri": uri,
                "name": "workspace"
            }])
        });

        let params = serde_json::json!({
            "processId": std::process::id(),
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {
                        "relatedInformation": true
                    },
                    "diagnostic": {
                        "dynamicRegistration": false
                    }
                },
                "workspace": {
                    "workspaceFolders": true,
                    "configuration": true
                }
            },
            "rootUri": root_uri,
            "workspaceFolders": workspace_folders
        });

        let _response = self.send_request("initialize", params)?;

        // Send initialized notification
        self.send_notification("initialized", serde_json::json!({}))?;

        Ok(())
    }

    /// Shutdown the LSP server
    pub fn shutdown(&mut self) -> Result<(), String> {
        // Send shutdown request but don't wait for response (server may exit immediately)
        let shutdown_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.request_id.fetch_add(1, Ordering::SeqCst),
            "method": "shutdown",
            "params": Value::Null
        });
        let _ = self.send_message(&shutdown_req);

        // Send exit notification
        let _ = self.send_notification("exit", Value::Null);

        // Give server a moment to exit gracefully, then kill if needed
        thread::sleep(Duration::from_millis(10));
        let _ = self.process.kill();
        let _ = self.process.wait();
        Ok(())
    }

    /// Find tsgo in local node_modules
    fn find_tsgo_in_local_node_modules(working_dir: Option<&str>) -> Option<String> {
        let base_dir = working_dir
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok())?;

        // Platform-specific path for @typescript/native-preview
        let platform_suffix = if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "darwin-arm64"
            } else {
                "darwin-x64"
            }
        } else if cfg!(target_os = "linux") {
            if cfg!(target_arch = "aarch64") {
                "linux-arm64"
            } else {
                "linux-x64"
            }
        } else if cfg!(target_os = "windows") {
            "win32-x64"
        } else {
            ""
        };

        // Helper to search for tsgo in a directory
        let search_in_dir = |dir: &std::path::Path| -> Option<String> {
            // Try pnpm structure first
            let pnpm_pattern = dir.join("node_modules/.pnpm");
            if pnpm_pattern.exists() {
                if let Ok(entries) = std::fs::read_dir(&pnpm_pattern) {
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.starts_with("@typescript+native-preview-")
                            && name_str.contains(platform_suffix)
                        {
                            let native_path = entry.path().join(&*cstr!(
                                "node_modules/@typescript/native-preview-{}/lib/tsgo",
                                platform_suffix
                            ));
                            if native_path.exists() {
                                return Some(native_path.to_string_lossy().into());
                            }
                        }
                    }
                }
            }

            // Try npm/yarn structure
            let native_candidates = [
                dir.join(&*cstr!(
                    "node_modules/@typescript/native-preview-{}/lib/tsgo",
                    platform_suffix
                )),
                dir.join("node_modules/@typescript/native-preview/lib/tsgo"),
            ];

            for candidate in &native_candidates {
                if candidate.exists() {
                    return Some(candidate.to_string_lossy().into());
                }
            }

            // Fallback to .bin/tsgo (requires Node.js in PATH)
            let candidates = [
                dir.join("node_modules/.bin/tsgo"),
                dir.join("node_modules/@typescript/native-preview/bin/tsgo"),
            ];

            for candidate in &candidates {
                if candidate.exists() {
                    return Some(candidate.to_string_lossy().into());
                }
            }

            None
        };

        // Search in base_dir first
        if let Some(path) = search_in_dir(&base_dir) {
            return Some(path);
        }

        // Walk up parent directories to find workspace root's node_modules
        let mut current = base_dir.as_path();
        while let Some(parent) = current.parent() {
            if let Some(path) = search_in_dir(parent) {
                return Some(path);
            }
            current = parent;
        }

        None
    }

    /// Find tsgo in common npm global install locations
    fn find_tsgo_in_common_locations() -> Option<String> {
        let home = std::env::var("HOME").ok()?;

        // Common npm global binary locations
        let candidates: [String; 10] = [
            // npm global (custom prefix)
            cstr!("{home}/.npm-global/bin/tsgo"),
            // npm global (default)
            cstr!("{home}/.npm/bin/tsgo"),
            // pnpm global
            cstr!("{home}/.local/share/pnpm/tsgo"),
            // volta
            cstr!("{home}/.volta/bin/tsgo"),
            // mise/asdf shims
            cstr!("{home}/.local/share/mise/shims/tsgo"),
            cstr!("{home}/.asdf/shims/tsgo"),
            // fnm
            cstr!("{home}/.local/share/fnm/node-versions/current/bin/tsgo"),
            // nvm (check current version)
            cstr!("{home}/.nvm/versions/node/current/bin/tsgo"),
            // Homebrew (macOS)
            "/opt/homebrew/bin/tsgo".into(),
            "/usr/local/bin/tsgo".into(),
        ];

        for path in candidates {
            if std::path::Path::new(path.as_str()).exists() {
                return Some(path);
            }
        }

        // Also try to get from npm root -g
        if let Ok(output) = std::process::Command::new("npm")
            .args(["root", "-g"])
            .output()
        {
            if output.status.success() {
                #[allow(clippy::disallowed_types)]
                let npm_root = std::string::String::from_utf8_lossy(&output.stdout);
                let npm_root = npm_root.trim();
                // npm root -g returns lib path, bin is sibling
                if let Some(lib_parent) = std::path::Path::new(npm_root).parent() {
                    let tsgo_path = lib_parent.join("bin/tsgo");
                    if tsgo_path.exists() {
                        return Some(tsgo_path.to_string_lossy().into());
                    }
                }
            }
        }

        None
    }
}

impl Drop for TsgoLspClient {
    fn drop(&mut self) {
        let _ = self.shutdown();
        // Clean up temporary tsconfig directory
        if let Some(ref dir) = self.temp_dir {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_temp_dir_base;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    fn unique_case_dir(name: &str) -> PathBuf {
        static NEXT_CASE_ID: AtomicUsize = AtomicUsize::new(0);

        let case_id = NEXT_CASE_ID.fetch_add(1, Ordering::Relaxed);
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("__agent_only")
            .join("tests")
            .join(format!(
                "tsgo-temp-dir-{name}-{}-{case_id}",
                std::process::id()
            ))
    }

    #[test]
    fn resolves_temp_dir_under_package_root_when_node_modules_exists() {
        let case_dir = unique_case_dir("package-root");
        let source_dir = case_dir.join("playground").join("src").join("shared");
        let node_modules_vue = case_dir.join("playground").join("node_modules").join("vue");

        let _ = fs::remove_dir_all(&case_dir);
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&node_modules_vue).unwrap();

        let resolved = resolve_temp_dir_base(Some(&source_dir));

        assert_eq!(
            resolved,
            case_dir
                .join("playground")
                .join("__agent_only")
                .join("vize-tsgo")
        );

        let _ = fs::remove_dir_all(&case_dir);
    }

    #[test]
    fn falls_back_to_nearest_available_node_modules_root() {
        let case_dir = unique_case_dir("fallback");
        let source_dir = case_dir.join("playground").join("src").join("shared");

        let _ = fs::remove_dir_all(&case_dir);
        fs::create_dir_all(&source_dir).unwrap();

        let resolved = resolve_temp_dir_base(Some(&source_dir));
        let expected_root = source_dir
            .ancestors()
            .find(|path| path.join("node_modules").join("vue").is_dir())
            .unwrap()
            .to_path_buf();

        assert_eq!(
            resolved,
            expected_root.join("__agent_only").join("vize-tsgo")
        );
        assert!(!resolved.starts_with(&source_dir));

        let _ = fs::remove_dir_all(&case_dir);
    }
}
