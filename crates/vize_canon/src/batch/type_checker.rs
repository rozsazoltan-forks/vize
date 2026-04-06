//! TypeChecker trait and BatchTypeChecker implementation.

use std::path::{Path, PathBuf};

use super::error::{CorsaError, CorsaResult};
use super::executor::CorsaExecutor;
use super::virtual_project::VirtualProject;
use super::Diagnostic;

/// Result of type checking.
#[derive(Debug, Default)]
pub struct TypeCheckResult {
    /// Diagnostics from type checking.
    pub diagnostics: Vec<Diagnostic>,
    /// Exit code from the Corsa process.
    pub exit_code: i32,
    /// Whether type checking succeeded.
    pub success: bool,
}

impl TypeCheckResult {
    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == 1)
    }

    /// Get the number of errors.
    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == 1).count()
    }

    /// Get the number of warnings.
    pub fn warning_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == 2).count()
    }
}

/// Trait for type checking.
pub trait TypeChecker: Send + Sync {
    /// Check the entire project.
    fn check_project(&self) -> CorsaResult<TypeCheckResult>;

    /// Check a single file.
    fn check_file(&self, path: &Path, content: &str) -> CorsaResult<Vec<Diagnostic>>;

    /// Check incrementally (only changed files).
    fn check_incremental(&self, changed: &[PathBuf]) -> CorsaResult<TypeCheckResult>;
}

/// Batch type checker using the Corsa CLI.
pub struct BatchTypeChecker {
    /// Virtual project.
    project: VirtualProject,
    /// Corsa executor.
    executor: CorsaExecutor,
    /// Whether the project has been scanned.
    scanned: bool,
}

impl BatchTypeChecker {
    /// Create a new batch type checker.
    pub fn new(project_root: &Path) -> CorsaResult<Self> {
        let project = VirtualProject::new(project_root)?;
        let executor = CorsaExecutor::new(project_root)?;

        Ok(Self {
            project,
            executor,
            scanned: false,
        })
    }

    /// Scan the project for source files.
    pub fn scan_project(&mut self) -> CorsaResult<()> {
        let project_root = self.project.project_root().to_path_buf();

        for entry in walkdir::WalkDir::new(&project_root)
            .into_iter()
            .filter_entry(|e| {
                // Don't filter the root directory itself
                if e.path() == project_root {
                    return true;
                }
                // Skip node_modules and hidden directories
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.') && name != "node_modules"
            })
        {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            match path.extension().and_then(|e| e.to_str()) {
                Some("vue") => {
                    let content = std::fs::read_to_string(path)?;
                    self.project.register_vue_file(path, &content)?;
                }
                Some("ts" | "tsx") => {
                    // Skip .d.ts files
                    if path.to_string_lossy().ends_with(".d.ts") {
                        continue;
                    }
                    self.project.register_ts_file(path)?;
                }
                _ => {}
            }
        }

        self.scanned = true;
        Ok(())
    }

    /// Get the number of registered files.
    pub fn file_count(&self) -> usize {
        self.project.file_count()
    }
}

impl TypeChecker for BatchTypeChecker {
    fn check_project(&self) -> CorsaResult<TypeCheckResult> {
        if !self.scanned {
            return Err(CorsaError::NotInitialized);
        }

        self.executor.check(&self.project)
    }

    fn check_file(&self, path: &Path, content: &str) -> CorsaResult<Vec<Diagnostic>> {
        // Create a temporary project with just this file
        let project_root = path.parent().unwrap_or(Path::new("."));
        let mut temp_project = VirtualProject::new(project_root)?;

        if path.extension().map(|e| e == "vue").unwrap_or(false) {
            temp_project.register_vue_file(path, content)?;
        } else {
            // For .ts files, we need to write it first
            std::fs::write(path, content)?;
            temp_project.register_ts_file(path)?;
        }

        let result = self.executor.check(&temp_project)?;
        Ok(result.diagnostics)
    }

    fn check_incremental(&self, changed: &[PathBuf]) -> CorsaResult<TypeCheckResult> {
        // For now, just do a full check
        // TODO: Implement proper incremental checking
        let _ = changed;
        self.check_project()
    }
}

#[cfg(test)]
mod tests;
