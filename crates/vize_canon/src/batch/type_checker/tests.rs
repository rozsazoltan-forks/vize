use super::{BatchTypeChecker, Diagnostic, TypeCheckResult};
use crate::batch::TypeChecker;
use std::path::PathBuf;
use tempfile::TempDir;
use vize_carton::{cstr, String};

#[test]
fn test_type_check_result() {
    let mut result = TypeCheckResult::default();
    assert!(!result.has_errors());
    assert_eq!(result.error_count(), 0);

    result.diagnostics.push(Diagnostic {
        file: PathBuf::from("test.vue"),
        line: 0,
        column: 0,
        message: "error".into(),
        code: Some(2304),
        severity: 1,
        block_type: None,
    });

    assert!(result.has_errors());
    assert_eq!(result.error_count(), 1);
}

#[test]
fn test_batch_type_checker_scan() {
    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let vue_content = r#"<template>
  <div>{{ message }}</div>
</template>

<script setup lang="ts">
const message = 'Hello'
</script>
"#;
    std::fs::write(src_dir.join("App.vue"), vue_content).unwrap();
    std::fs::write(src_dir.join("utils.ts"), "export const foo = 'bar';").unwrap();

    let mut checker = match BatchTypeChecker::new(temp_dir.path()) {
        Ok(checker) => checker,
        Err(_) => return,
    };

    checker.scan_project().unwrap();
    assert_eq!(checker.file_count(), 2);
}

#[test]
fn batch_type_checker_snapshots_vue_diagnostics() {
    let temp_dir = TempDir::new().unwrap();
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        temp_dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "noEmit": true
  },
  "include": ["src/**/*.ts", "src/**/*.vue"]
}"#,
    )
    .unwrap();
    std::fs::write(
        src_dir.join("App.vue"),
        r#"<script setup lang="ts">
const count: number = 'oops'
</script>
"#,
    )
    .unwrap();

    let mut checker = match BatchTypeChecker::new(temp_dir.path()) {
        Ok(checker) => checker,
        Err(_) => return,
    };
    checker.scan_project().unwrap();

    let result = checker.check_project().unwrap();
    let snapshot: Vec<_> = result
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                relative_path(temp_dir.path(), &diagnostic.file),
                diagnostic.code,
                diagnostic.line,
                diagnostic.column,
                diagnostic.message.clone(),
                diagnostic.block_type,
            )
        })
        .collect();

    insta::assert_debug_snapshot!("batch_type_checker_vue_diagnostics", snapshot);
}

fn relative_path(root: &std::path::Path, file: &std::path::Path) -> String {
    file.strip_prefix(root)
        .map(|path| cstr!("{}", path.display()))
        .unwrap_or_else(|_| cstr!("{}", file.display()))
}
