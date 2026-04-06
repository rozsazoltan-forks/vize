//! Virtual project management for Corsa-backed type checking.
//!
//! This module manages the virtual TypeScript project in `node_modules/.vize/canon/`.

use std::path::{Path, PathBuf};

use super::error::{CorsaError, CorsaResult};
use super::import_rewriter::ImportRewriter;
use super::source_map::CompositeSourceMap;
use super::virtual_ts::VirtualTsGenerator;
use super::SfcBlockType;
use oxc_span::SourceType;
use vize_carton::cstr;
use vize_carton::FxHashMap;
use vize_carton::String;

/// A virtual file in the project.
#[derive(Debug)]
pub struct VirtualFile {
    /// Generated content.
    pub content: String,
    /// Source map for position mapping.
    pub source_map: CompositeSourceMap,
    /// Original file path.
    pub original_path: PathBuf,
}

/// Original position after mapping.
#[derive(Debug, Clone)]
pub struct OriginalPosition {
    /// Original file path.
    pub path: PathBuf,
    /// Line number (0-based).
    pub line: u32,
    /// Column number (0-based).
    pub column: u32,
    /// SFC block type if applicable.
    pub block_type: Option<SfcBlockType>,
}

/// Virtual project for Corsa-backed type checking.
pub struct VirtualProject {
    /// Project root directory.
    project_root: PathBuf,

    /// Virtual project root (node_modules/.vize/canon).
    virtual_root: PathBuf,

    /// Virtual files.
    virtual_files: FxHashMap<PathBuf, VirtualFile>,

    /// Virtual TypeScript generator.
    generator: VirtualTsGenerator,

    /// Import rewriter.
    rewriter: ImportRewriter,
}

impl VirtualProject {
    /// Create a new virtual project.
    pub fn new(project_root: &Path) -> CorsaResult<Self> {
        let virtual_root = project_root
            .join("node_modules")
            .join(".vize")
            .join("canon");

        Ok(Self {
            project_root: project_root.to_path_buf(),
            virtual_root,
            virtual_files: FxHashMap::default(),
            generator: VirtualTsGenerator::new(),
            rewriter: ImportRewriter::new(),
        })
    }

    /// Get the project root.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the virtual root.
    pub fn virtual_root(&self) -> &Path {
        &self.virtual_root
    }

    /// Register a .vue file.
    pub fn register_vue_file(&mut self, path: &Path, content: &str) -> CorsaResult<()> {
        let result = self
            .generator
            .generate_from_content(content)
            .map_err(CorsaError::SfcParse)?;

        // Calculate virtual path: project/src/App.vue -> .vize/canon/src/App.vue.ts
        let relative = path.strip_prefix(&self.project_root)?;
        let mut virtual_path = self.virtual_root.join(relative);

        // Change extension from .vue to .vue.ts
        let file_name = virtual_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| cstr!("{n}.ts"))
            .ok_or_else(|| CorsaError::PathError {
                path: path.to_path_buf(),
            })?;
        virtual_path.set_file_name(file_name);

        self.virtual_files.insert(
            virtual_path,
            VirtualFile {
                content: result.code,
                source_map: CompositeSourceMap::new(
                    Some(result.source_map),
                    super::import_rewriter::ImportSourceMap::empty(),
                ),
                original_path: path.to_path_buf(),
            },
        );

        Ok(())
    }

    /// Register a .ts or .tsx file.
    pub fn register_ts_file(&mut self, path: &Path) -> CorsaResult<()> {
        let content = std::fs::read_to_string(path)?;

        let source_type = if path.extension().map(|e| e == "tsx").unwrap_or(false) {
            SourceType::tsx()
        } else {
            SourceType::ts()
        };

        let result = self.rewriter.rewrite(&content, source_type);

        let relative = path.strip_prefix(&self.project_root)?;
        let virtual_path = self.virtual_root.join(relative);

        self.virtual_files.insert(
            virtual_path,
            VirtualFile {
                content: result.code,
                source_map: CompositeSourceMap::new(None, result.source_map),
                original_path: path.to_path_buf(),
            },
        );

        Ok(())
    }

    /// Materialize the virtual project to disk.
    pub fn materialize(&self) -> CorsaResult<()> {
        // 1. Create/clean the virtual root
        if self.virtual_root.exists() {
            std::fs::remove_dir_all(&self.virtual_root)?;
        }
        std::fs::create_dir_all(&self.virtual_root)?;

        // 2. Write all virtual files
        for (path, file) in &self.virtual_files {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &file.content)?;
        }

        // 3. Generate tsconfig.json
        let tsconfig = self.generate_tsconfig()?;
        std::fs::write(self.virtual_root.join("tsconfig.json"), tsconfig)?;

        Ok(())
    }

    /// Generate tsconfig.json for the virtual project.
    fn generate_tsconfig(&self) -> CorsaResult<String> {
        // Read the original tsconfig.json if it exists
        let original_tsconfig = self.project_root.join("tsconfig.json");
        let paths = if original_tsconfig.exists() {
            self.extract_paths_from_tsconfig(&original_tsconfig)?
        } else {
            serde_json::json!({})
        };

        let config = serde_json::json!({
            "compilerOptions": {
                "target": "ESNext",
                "module": "ESNext",
                "moduleResolution": "bundler",
                "strict": true,
                "noEmit": true,
                "skipLibCheck": true,
                "baseUrl": ".",
                "paths": paths
            },
            "include": ["**/*.ts"],
            "exclude": []
        });

        Ok(serde_json::to_string_pretty(&config)?.into())
    }

    /// Extract paths configuration from original tsconfig.json.
    fn extract_paths_from_tsconfig(&self, tsconfig_path: &Path) -> CorsaResult<serde_json::Value> {
        let content = std::fs::read_to_string(tsconfig_path)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;

        // Extract paths from compilerOptions
        if let Some(paths) = config.get("compilerOptions").and_then(|c| c.get("paths")) {
            return Ok(paths.clone());
        }

        Ok(serde_json::json!({}))
    }

    /// Map a virtual position to the original position.
    pub fn map_to_original(
        &self,
        virtual_path: &Path,
        line: u32,
        column: u32,
    ) -> Option<OriginalPosition> {
        let file = self.virtual_files.get(virtual_path)?;

        // Convert line/column to offset in virtual TS
        let virtual_offset = super::source_map::line_col_to_offset(&file.content, line, column)?;

        // Map through composite source map
        let (orig_offset, _, block_type) = file.source_map.get_original_position(virtual_offset)?;

        // Read original file content to convert offset to line/column
        let original_content = std::fs::read_to_string(&file.original_path).ok()?;
        let (orig_line, orig_col) =
            super::source_map::offset_to_line_col(&original_content, orig_offset)?;

        Some(OriginalPosition {
            path: file.original_path.clone(),
            line: orig_line,
            column: orig_col,
            block_type,
        })
    }

    /// Map an original position to the virtual position.
    pub fn map_to_virtual(
        &self,
        original_path: &Path,
        line: u32,
        column: u32,
    ) -> Option<(PathBuf, u32, u32)> {
        // Find the virtual file for this original path
        for (virtual_path, file) in &self.virtual_files {
            if file.original_path == original_path {
                // Read original content to convert line/col to offset
                let original_content = std::fs::read_to_string(&file.original_path).ok()?;
                let sfc_offset =
                    super::source_map::line_col_to_offset(&original_content, line, column)?;

                // Determine block type heuristic: if the file has an SFC map, try Script first
                if let Some(ref sfc_map) = file.source_map.sfc_map {
                    // Try ScriptSetup first, then Script, then Template
                    for block_type in [
                        super::SfcBlockType::ScriptSetup,
                        super::SfcBlockType::Script,
                        super::SfcBlockType::Template,
                    ] {
                        if let Some(virtual_offset) =
                            sfc_map.get_virtual_offset(sfc_offset, block_type)
                        {
                            // Apply import rewrite mapping
                            let final_offset = file
                                .source_map
                                .import_map
                                .get_virtual_offset(virtual_offset);
                            // Convert virtual offset to line/col
                            if let Some((vline, vcol)) =
                                super::source_map::offset_to_line_col(&file.content, final_offset)
                            {
                                return Some((virtual_path.clone(), vline, vcol));
                            }
                        }
                    }
                }

                // Fallback for .ts files: only import rewrite mapping
                let final_offset = file.source_map.import_map.get_virtual_offset(sfc_offset);
                if let Some((vline, vcol)) =
                    super::source_map::offset_to_line_col(&file.content, final_offset)
                {
                    return Some((virtual_path.clone(), vline, vcol));
                }

                // Final fallback: pass through
                return Some((virtual_path.clone(), line, column));
            }
        }
        None
    }

    /// Get the number of registered files.
    pub fn file_count(&self) -> usize {
        self.virtual_files.len()
    }

    /// Check if the project has any files.
    pub fn is_empty(&self) -> bool {
        self.virtual_files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::VirtualProject;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_virtual_project_new() {
        let temp_dir = TempDir::new().unwrap();
        let project = VirtualProject::new(temp_dir.path()).unwrap();

        assert_eq!(project.project_root(), temp_dir.path());
        assert!(project.virtual_root().ends_with("node_modules/.vize/canon"));
    }

    #[test]
    fn test_register_vue_file() {
        let temp_dir = TempDir::new().unwrap();
        let mut project = VirtualProject::new(temp_dir.path()).unwrap();

        let vue_content = r#"<template>
  <div>{{ message }}</div>
</template>

<script setup lang="ts">
const message = 'Hello'
</script>
"#;

        // Create source file
        let src_dir = temp_dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let vue_path = src_dir.join("App.vue");
        fs::write(&vue_path, vue_content).unwrap();

        project.register_vue_file(&vue_path, vue_content).unwrap();

        assert_eq!(project.file_count(), 1);
    }

    #[test]
    fn test_materialize() {
        let temp_dir = TempDir::new().unwrap();
        let mut project = VirtualProject::new(temp_dir.path()).unwrap();

        let vue_content = r#"<template>
  <div>{{ message }}</div>
</template>

<script setup lang="ts">
const message = 'Hello'
</script>
"#;

        let src_dir = temp_dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let vue_path = src_dir.join("App.vue");
        fs::write(&vue_path, vue_content).unwrap();

        project.register_vue_file(&vue_path, vue_content).unwrap();
        project.materialize().unwrap();

        // Check that virtual files were created
        let virtual_file = temp_dir
            .path()
            .join("node_modules/.vize/canon/src/App.vue.ts");
        assert!(virtual_file.exists());

        let tsconfig = temp_dir
            .path()
            .join("node_modules/.vize/canon/tsconfig.json");
        assert!(tsconfig.exists());
    }
}
