//! Corsa-backed batch executor.
//!
//! This module materializes the virtual project, asks the Corsa project-session
//! API for diagnostics across every generated file, and maps those diagnostics
//! back to the original source positions.

use std::path::{Path, PathBuf};

use super::error::{CorsaError, CorsaNotFoundError, CorsaResult};
use super::type_checker::TypeCheckResult;
use super::virtual_project::VirtualProject;
use crate::corsa_client::CorsaProjectClient;
use vize_carton::{cstr, String};

mod diagnostics;

use diagnostics::map_batch_diagnostics;

/// Batch executor backed by `corsa`'s project-session diagnostics API.
pub struct CorsaExecutor {
    /// Path to the resolved Corsa executable.
    corsa_path: PathBuf,
}

impl CorsaExecutor {
    /// Create a new executor by finding a local or global Corsa executable.
    pub fn new(project_root: &Path) -> Result<Self, CorsaNotFoundError> {
        for executable in ["corsa", "tsgo"] {
            let local_corsa = project_root.join("node_modules/.bin").join(executable);
            if local_corsa.exists() {
                return Ok(Self {
                    corsa_path: local_corsa,
                });
            }
        }

        for executable in ["corsa", "tsgo"] {
            if let Ok(global_corsa) = which::which(executable) {
                return Ok(Self {
                    corsa_path: global_corsa,
                });
            }
        }

        if let Some(mise_corsa) = Self::find_mise_corsa() {
            return Ok(Self {
                corsa_path: mise_corsa,
            });
        }

        Err(CorsaNotFoundError::new(project_root))
    }

    /// Get the resolved executable path.
    pub fn corsa_path(&self) -> &Path {
        &self.corsa_path
    }

    /// Run type checking on the virtual project.
    pub fn check(&self, project: &VirtualProject) -> CorsaResult<TypeCheckResult> {
        project.materialize()?;

        let corsa_path = self.corsa_path.to_string_lossy();
        let mut client = CorsaProjectClient::new_for_workspace(
            Some(corsa_path.as_ref()),
            project.virtual_root(),
        )
        .map_err(map_corsa_error)?;
        let uris = collect_virtual_file_uris(project.virtual_root())?;
        let diagnostics = map_batch_diagnostics(client.request_diagnostics_batch(&uris), project);
        let success = diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != 1);

        Ok(TypeCheckResult {
            exit_code: if success { 0 } else { 1 },
            success,
            diagnostics,
        })
    }

    /// Find Corsa in mise shims directory.
    fn find_mise_corsa() -> Option<PathBuf> {
        let mise_data_dir = std::env::var("MISE_DATA_DIR")
            .map(PathBuf::from)
            .ok()
            .or_else(|| dirs::data_local_dir().map(|d| d.join("mise")))?;

        for executable in ["corsa", "tsgo"] {
            let shims_corsa = mise_data_dir.join("shims").join(executable);
            if shims_corsa.exists() {
                return Some(shims_corsa);
            }
        }

        if let Some(xdg_data) = std::env::var("XDG_DATA_HOME").ok().map(PathBuf::from) {
            for executable in ["corsa", "tsgo"] {
                let xdg_corsa = xdg_data.join("mise").join("shims").join(executable);
                if xdg_corsa.exists() {
                    return Some(xdg_corsa);
                }
            }
        }

        if let Some(home) = dirs::home_dir() {
            for executable in ["corsa", "tsgo"] {
                let home_corsa = home.join(".local/share/mise/shims").join(executable);
                if home_corsa.exists() {
                    return Some(home_corsa);
                }
            }
        }

        None
    }
}

fn collect_virtual_file_uris(virtual_root: &Path) -> CorsaResult<Vec<String>> {
    let mut uris = Vec::new();

    for entry in walkdir::WalkDir::new(virtual_root) {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some("ts" | "tsx") = path.extension().and_then(|extension| extension.to_str()) {
            uris.push(cstr!("file://{}", path.display()));
        }
    }

    uris.sort();
    Ok(uris)
}

fn map_corsa_error(message: String) -> CorsaError {
    CorsaError::CorsaExecution {
        exit_code: -1,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::collect_virtual_file_uris;
    use std::fs;
    use tempfile::TempDir;
    use vize_carton::cstr;

    #[test]
    fn collects_virtual_type_script_files_only() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        fs::write(root.join("index.ts"), "").unwrap();
        fs::write(root.join("component.vue.ts"), "").unwrap();
        fs::write(root.join("tsconfig.json"), "{}").unwrap();
        fs::write(root.join("ignored.js"), "").unwrap();

        let uris = collect_virtual_file_uris(root).unwrap();

        assert_eq!(
            uris,
            vec![
                cstr!("file://{}", root.join("component.vue.ts").display()),
                cstr!("file://{}", root.join("index.ts").display()),
            ]
        );
    }
}
