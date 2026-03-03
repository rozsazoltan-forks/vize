//! Check command execution logic.
//!
//! Contains the direct tsgo LSP runner, Unix socket runner, file collection,
//! and globals parsing.

#![allow(clippy::disallowed_macros)]

use std::{fs, path::PathBuf, time::Instant};

use ignore::WalkBuilder;
use vize_carton::cstr;
use vize_carton::ToCompactString;

use super::{
    reporting::{JsonFileResult, JsonOutput},
    CheckArgs, GeneratedFile,
};

/// Run type checking via Unix socket connection to check-server.
#[cfg(unix)]
pub(crate) fn run_with_socket(args: &CheckArgs, socket_path: &str) {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixStream,
    };

    use super::{JsonRpcResponse, ServerCheckResult};

    let start = Instant::now();

    // Collect files
    let files: Vec<PathBuf> = collect_vue_files(&args.patterns);

    if files.is_empty() {
        eprintln!("No .vue files found matching patterns: {:?}", args.patterns);
        return;
    }

    // Connect to server
    let mut stream = match UnixStream::connect(socket_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "\x1b[31mError:\x1b[0m Failed to connect to check-server: {}",
                e
            );
            eprintln!();
            eprintln!("\x1b[33mHint:\x1b[0m Start the server first:");
            eprintln!("  vize check-server --socket {}", socket_path);
            std::process::exit(1);
        }
    };

    if !args.quiet {
        eprintln!("Connected to check-server at {}", socket_path);
        eprintln!("Type checking {} files...", files.len());
    }

    let mut total_errors = 0;
    #[allow(clippy::disallowed_types, clippy::disallowed_methods)]
    let mut results: Vec<(std::string::String, ServerCheckResult)> = Vec::new();

    for path in &files {
        #[allow(clippy::disallowed_types)]
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to read {}: {}", path.display(), e);
                continue;
            }
        };

        #[allow(clippy::disallowed_methods)]
        let filename = path.to_string_lossy().to_string();

        // Send request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "check",
            "params": {
                "uri": filename,
                "content": source
            }
        });

        if writeln!(stream, "{}", request).is_err() {
            eprintln!("Failed to send request");
            break;
        }
        if stream.flush().is_err() {
            eprintln!("Failed to flush");
            break;
        }

        // Read response
        let mut reader = BufReader::new(&stream);
        #[allow(clippy::disallowed_types)]
        let mut response_line = std::string::String::new();
        if reader.read_line(&mut response_line).is_err() {
            eprintln!("Failed to read response");
            break;
        }

        let response: JsonRpcResponse = match serde_json::from_str(&response_line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to parse response: {}", e);
                continue;
            }
        };

        if let Some(error) = response.error {
            eprintln!("Server error: {}", error.message);
            continue;
        }

        if let Some(result) = response.result {
            total_errors += result.error_count;

            if args.show_virtual_ts {
                eprintln!("\n=== {} ===", filename);
                eprintln!("{}", result.virtual_ts);
            }

            results.push((filename, result));
        }
    }

    let total_time = start.elapsed();

    // Print results
    if !args.quiet {
        for (filename, result) in &results {
            if result.diagnostics.is_empty() {
                continue;
            }

            println!("\n\x1b[4m{}\x1b[0m", filename);
            for diag in &result.diagnostics {
                let color = if diag.severity == "error" {
                    "\x1b[31m"
                } else {
                    "\x1b[33m"
                };
                let code_str = diag
                    .code
                    .as_ref()
                    .map(|c| format!(" [{}]", c))
                    .unwrap_or_default();
                println!(
                    "  {}{}:{}:{}\x1b[0m{} {}",
                    color, diag.severity, diag.line, diag.column, code_str, diag.message
                );
            }
        }
    }

    // Print summary
    let status = if total_errors > 0 {
        "\x1b[31m\u{2717}\x1b[0m"
    } else {
        "\x1b[32m\u{2713}\x1b[0m"
    };
    println!(
        "\n{} Type checked {} files in {:.2?} (via socket)",
        status,
        files.len(),
        total_time
    );

    if total_errors > 0 {
        println!("  \x1b[31m{} error(s)\x1b[0m", total_errors);
        std::process::exit(1);
    } else {
        println!("  \x1b[32mNo type errors found!\x1b[0m");
    }
}

/// Run type checking directly with tsgo LSP (no file I/O).
pub(crate) fn run_direct(args: &CheckArgs) {
    use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
    use vize_atelier_core::parser::parse;
    use vize_atelier_sfc::{parse_sfc, SfcParseOptions};
    use vize_canon::{
        lsp_client::TsgoLspClient,
        virtual_ts::{generate_virtual_ts_with_offsets, VirtualTsOptions},
    };
    use vize_carton::Bump;
    use vize_croquis::{Analyzer, AnalyzerOptions, ImportStatementInfo, ReExportInfo, TypeExport};

    use super::reporting::map_diagnostic_position;

    let start = Instant::now();

    // Load vize.config.json and write JSON Schema
    let config = crate::config::load_config(None);
    crate::config::write_schema(None);

    // Build VirtualTsOptions from config (.d.ts file) or default (empty).
    let mut vts_options = if let Some(ref dts_path) = config.check.globals {
        let resolved = std::path::Path::new(dts_path);
        match parse_dts_globals(resolved) {
            Ok(globals) => VirtualTsOptions {
                template_globals: globals,
                ..Default::default()
            },
            Err(e) => {
                eprintln!(
                    "\x1b[33mWarning:\x1b[0m Failed to parse globals from {}: {}",
                    dts_path, e
                );
                VirtualTsOptions::default()
            }
        }
    } else {
        VirtualTsOptions::default()
    };

    // Detect Nuxt project and add auto-import stubs
    detect_nuxt_auto_imports(&mut vts_options);

    // Collect .vue files
    let collect_start = Instant::now();
    let files = collect_vue_files(&args.patterns);
    let collect_time = collect_start.elapsed();

    if files.is_empty() {
        eprintln!("No .vue files found matching patterns: {:?}", args.patterns);
        return;
    }

    if !args.quiet {
        eprintln!("Generating Virtual TypeScript for {} files...", files.len());
    }

    let gen_start = Instant::now();

    // Generate Virtual TypeScript for each file (in parallel)
    let generated: Vec<GeneratedFile> = files
        .par_iter()
        .filter_map(|path| {
            let source = fs::read_to_string(path).ok()?;
            let original_content = source.clone();
            // Use absolute path for proper file:// URI
            let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            #[allow(clippy::disallowed_methods)]
            let filename = abs_path.to_string_lossy().to_string();

            // Parse SFC
            let parse_opts = SfcParseOptions {
                filename: filename.clone().into(),
                ..Default::default()
            };
            let descriptor = parse_sfc(&source, parse_opts).ok()?;

            // Extract CSS module names from <style module> blocks
            // Parser sets module to "$style" for bare `<style module>`,
            // or the attribute value for `<style module="custom">`
            let css_modules: Vec<vize_carton::String> = descriptor
                .styles
                .iter()
                .filter_map(|style| style.module.as_ref().map(|m| m.to_compact_string()))
                .collect();

            // Build per-file options with CSS modules
            let file_vts_options = if css_modules.is_empty() {
                None
            } else {
                let mut opts = vts_options.clone();
                opts.css_modules = css_modules;
                Some(opts)
            };
            let effective_options = file_vts_options.as_ref().unwrap_or(&vts_options);

            // Get script content (combine both script and script setup if both exist)
            let (script_content, script_offset): (Option<vize_carton::String>, u32) =
                match (descriptor.script.as_ref(), descriptor.script_setup.as_ref()) {
                    (Some(script), Some(script_setup)) => {
                        // Both exist: combine them (plain script first, then script setup)
                        (
                            Some(cstr!("{}\n{}", script.content, script_setup.content)),
                            script.loc.start as u32,
                        )
                    }
                    (None, Some(script_setup)) => (
                        Some(script_setup.content.to_compact_string()),
                        script_setup.loc.start as u32,
                    ),
                    (Some(script), None) => (
                        Some(script.content.to_compact_string()),
                        script.loc.start as u32,
                    ),
                    (None, None) => (None, 0),
                };
            let script_content_ref = script_content.as_deref();

            // Create allocator
            let allocator = Bump::new();

            // Analyze - need to analyze both script and script_setup if both exist
            let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
            let has_both_scripts = descriptor.script.is_some() && descriptor.script_setup.is_some();

            // Analyze plain script first (exports types, interfaces, etc.)
            if let Some(ref script) = descriptor.script {
                analyzer.analyze_script_plain(&script.content);
            }

            // Save plain script's module-level spans before setup analysis overwrites them
            let plain_spans: Option<(
                Vec<ImportStatementInfo>,
                Vec<ReExportInfo>,
                Vec<TypeExport>,
            )> = if has_both_scripts {
                Some((
                    analyzer.summary().import_statements.clone(),
                    analyzer.summary().re_exports.clone(),
                    analyzer.summary().type_exports.clone(),
                ))
            } else {
                None
            };

            // Then analyze script setup (reactive bindings, macros, etc.)
            if let Some(ref script_setup) = descriptor.script_setup {
                let generic = script_setup.attrs.get("generic").map(|s| s.as_ref());
                analyzer.analyze_script_setup_with_generic(&script_setup.content, generic);
            }

            let template_offset: u32 = descriptor
                .template
                .as_ref()
                .map(|t| t.loc.start as u32)
                .unwrap_or(0);

            let template_ast = if let Some(ref template) = descriptor.template {
                let (root, _) = parse(&allocator, &template.content);
                analyzer.analyze_template(&root);
                Some(root)
            } else {
                None
            };

            let mut summary = analyzer.finish();

            // When both script blocks exist, the combined content is
            // "{script.content}\n{setup.content}" but Croquis spans are relative
            // to each block individually. Adjust setup spans and merge plain spans.
            if let (Some((plain_imports, plain_reexports, plain_types)), Some(script)) =
                (plain_spans, descriptor.script.as_ref())
            {
                let plain_len = script.content.len() as u32 + 1; // +1 for \n separator
                                                                 // Croquis currently has setup spans (relative to setup content).
                                                                 // Shift them to be relative to the combined content.
                for imp in &mut summary.import_statements {
                    imp.start += plain_len;
                    imp.end += plain_len;
                }
                for re in &mut summary.re_exports {
                    re.start += plain_len;
                    re.end += plain_len;
                }
                for te in &mut summary.type_exports {
                    te.start += plain_len;
                    te.end += plain_len;
                }
                // Merge plain script's spans (already at offset 0 in combined content)
                summary.import_statements.extend(plain_imports);
                summary.re_exports.extend(plain_reexports);
                summary.type_exports.extend(plain_types);
            }

            // Generate Virtual TS using canon's implementation
            let output = generate_virtual_ts_with_offsets(
                &summary,
                script_content_ref,
                template_ast.as_ref(),
                script_offset,
                template_offset,
                effective_options,
            );

            Some(GeneratedFile {
                original: filename,
                virtual_ts: output.code.into(),
                source_map: output.mappings,
                original_content,
            })
        })
        .collect();

    let gen_time = gen_start.elapsed();

    if generated.is_empty() {
        eprintln!("No files to check");
        return;
    }

    if args.show_virtual_ts {
        for g in &generated {
            eprintln!("\n=== {} ===", g.original);
            eprintln!("{}", g.virtual_ts);
        }
    }

    // Profile mode: write Virtual TS and timing to node_modules/.vize directory
    if args.profile {
        let profile_dir = PathBuf::from("node_modules/.vize/check-profile");
        if let Err(e) = fs::create_dir_all(&profile_dir) {
            eprintln!("Failed to create profile directory: {}", e);
        } else {
            for g in &generated {
                let file_name = PathBuf::from(&g.original)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_compact_string())
                    .unwrap_or_else(|| "unknown".into());
                let ts_path = profile_dir.join(format!("{}.ts", file_name));
                if let Err(e) = fs::write(&ts_path, &g.virtual_ts) {
                    eprintln!("Failed to write {}: {}", ts_path.display(), e);
                }
            }
            eprintln!(
                "\x1b[33mProfile:\x1b[0m Virtual TS files written to {}",
                profile_dir.display()
            );
        }
    }

    if !args.quiet {
        eprintln!("Running tsgo LSP on {} files...", generated.len());
    }

    let check_start = Instant::now();

    // Find project root from first generated file (for tsconfig resolution)
    // Skip .nuxt, .out, node_modules directories when looking for the main tsconfig
    let project_root = generated
        .first()
        .map(|g| std::path::Path::new(&g.original))
        .and_then(|p| {
            // Walk up to find directory containing tsconfig.json
            // that is NOT in a generated/hidden directory
            let mut dir = p.parent();
            let mut best_tsconfig: Option<std::path::PathBuf> = None;

            while let Some(d) = dir {
                let dir_name = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let is_generated_dir = dir_name.starts_with('.')
                    || dir_name == "node_modules"
                    || dir_name == "dist"
                    || dir_name == "build";

                if d.join("tsconfig.json").exists() {
                    if is_generated_dir {
                        // Keep looking for a better one
                        if best_tsconfig.is_none() {
                            best_tsconfig = Some(d.to_path_buf());
                        }
                    } else {
                        // Found a tsconfig in a non-generated directory - use it
                        return Some(d.to_string_lossy().to_compact_string());
                    }
                }
                dir = d.parent();
            }

            // Use the best found tsconfig (even if in generated dir) or fallback
            if let Some(d) = best_tsconfig {
                return Some(d.to_string_lossy().to_compact_string());
            }

            // Fallback: use directory of the first file
            p.parent().map(|d| d.to_string_lossy().to_compact_string())
        });

    // Build shared URI map for all files (so imports can be resolved across servers)
    #[allow(clippy::disallowed_types)]
    let uri_map: Vec<(std::string::String, std::string::String)> = generated
        .iter()
        .map(|g| {
            let virtual_uri = format!("file://{}.mts", g.original);
            (virtual_uri, g.virtual_ts.clone())
        })
        .collect();

    // Determine number of parallel LSP servers
    // Only use parallel servers for large file counts (threshold: 30 files)
    // Below this threshold, the overhead of multiple servers negates the benefit
    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let num_servers = if generated.len() < 30 {
        1 // Single server for small projects (less overhead)
    } else {
        // Use at most 4 servers (diminishing returns beyond that)
        num_cpus.min(4).min(generated.len() / 10).max(1)
    };

    // Partition INDICES for diagnostics collection (each server checks a subset)
    let chunk_size = generated.len().div_ceil(num_servers);
    let index_chunks: Vec<_> = (0..generated.len())
        .collect::<Vec<_>>()
        .chunks(chunk_size)
        .map(|c| c.to_vec())
        .collect();

    // Run type checking in parallel across multiple LSP servers
    use std::sync::{
        atomic::{AtomicUsize, Ordering as AtomicOrdering},
        Mutex,
    };
    let total_errors = AtomicUsize::new(0);
    #[allow(clippy::disallowed_types)]
    let all_diagnostics: Mutex<Vec<(std::string::String, Vec<std::string::String>)>> =
        Mutex::new(Vec::new());

    std::thread::scope(|s| {
        let handles: Vec<_> = index_chunks
            .into_iter()
            .map(|indices| {
                let project_root = project_root.clone();
                let tsgo_path = args.tsgo_path.clone();
                let total_errors = &total_errors;
                let all_diagnostics = &all_diagnostics;
                let uri_map = &uri_map;
                let generated = &generated;

                s.spawn(move || {
                    // Initialize LSP client for this thread
                    let mut lsp_client =
                        match TsgoLspClient::new(tsgo_path.as_deref(), project_root.as_deref()) {
                            Ok(client) => client,
                            Err(e) => {
                                eprintln!("\x1b[31mError:\x1b[0m Failed to start tsgo LSP: {}", e);
                                return;
                            }
                        };

                    // PHASE 1: Open files
                    // For single server: open all files
                    // For multiple servers: only open assigned files (rely on tsconfig for imports)
                    let files_to_open: Vec<_> = if num_servers == 1 {
                        uri_map.iter().collect()
                    } else {
                        indices.iter().map(|i| &uri_map[*i]).collect()
                    };

                    for (uri, content) in &files_to_open {
                        let _ = lsp_client.did_open_fast(uri, content);
                    }

                    // Wait for diagnostics
                    lsp_client.wait_for_diagnostics(files_to_open.len());

                    // PHASE 2: Request diagnostics in batch (pipelined)
                    // tsgo doesn't publish diagnostics automatically - we must request them
                    let uris: Vec<vize_carton::String> = indices
                        .iter()
                        .map(|i| cstr!("file://{}.mts", generated[*i].original))
                        .collect();

                    let batch_results = lsp_client.request_diagnostics_batch(&uris);

                    // Build a map from URI to diagnostics
                    let diag_map: vize_carton::FxHashMap<_, _> =
                        batch_results.into_iter().collect();

                    #[allow(clippy::disallowed_types)]
                    let mut chunk_diagnostics: Vec<(
                        std::string::String,
                        Vec<std::string::String>,
                    )> = Vec::new();

                    for idx in &indices {
                        let g = &generated[*idx];
                        let virtual_uri = cstr!("file://{}.mts", g.original);

                        // Get diagnostics from batch result
                        let diagnostics = diag_map
                            .get(virtual_uri.as_str())
                            .cloned()
                            .unwrap_or_default();

                        // Filter and format diagnostics
                        #[allow(clippy::disallowed_types)]
                        let mut file_diags: Vec<std::string::String> = Vec::new();
                        for diag in &diagnostics {
                            let code_num = diag.code.as_ref().and_then(|c| match c {
                                serde_json::Value::Number(n) => n.as_u64(),
                                serde_json::Value::String(s) => {
                                    // Handle both "2307" and "TS2307" formats
                                    let stripped = s.strip_prefix("TS").unwrap_or(s);
                                    stripped.parse::<u64>().ok()
                                }
                                _ => None,
                            });

                            // Module resolution: fundamental limitation of single-file mode.
                            // tsgo cannot resolve .vue imports, path aliases, or npm packages
                            // without a full project context. This is NOT a virtual TS bug.
                            if matches!(code_num, Some(2307) | Some(2666)) {
                                continue;
                            }

                            let severity = match diag.severity {
                                Some(1) => {
                                    total_errors.fetch_add(1, AtomicOrdering::Relaxed);
                                    "error"
                                }
                                Some(2) => "warning",
                                _ => {
                                    total_errors.fetch_add(1, AtomicOrdering::Relaxed);
                                    "error"
                                }
                            };
                            #[allow(clippy::disallowed_types)]
                            let code_str = diag
                                .code
                                .as_ref()
                                .map(|c| match c {
                                    serde_json::Value::Number(n) => format!(" [TS{}]", n),
                                    serde_json::Value::String(s) => format!(" [{}]", s),
                                    _ => std::string::String::new(),
                                })
                                .unwrap_or_default();
                            // Map virtual TS position -> SFC position
                            let (line, col) = map_diagnostic_position(
                                &g.virtual_ts,
                                &g.source_map,
                                &g.original_content,
                                diag.range.start.line,
                                diag.range.start.character,
                            );
                            file_diags.push(format!(
                                "{}:{}:{}{} {}",
                                severity, line, col, code_str, diag.message
                            ));
                        }

                        if !file_diags.is_empty() {
                            chunk_diagnostics.push((g.original.clone(), file_diags));
                        }
                    }

                    // PHASE 3: Close files that were opened
                    for (uri, _) in &files_to_open {
                        let _ = lsp_client.did_close(uri);
                    }

                    // Merge diagnostics into shared state
                    if let Ok(mut diags) = all_diagnostics.lock() {
                        diags.extend(chunk_diagnostics);
                    }
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    });

    let total_errors = total_errors.load(AtomicOrdering::Relaxed);
    let all_diagnostics = all_diagnostics.into_inner().unwrap();

    let check_time = check_start.elapsed();
    let total_time = start.elapsed();

    // JSON output
    if args.format == "json" {
        let mut files: Vec<JsonFileResult> = generated
            .iter()
            .map(|g| {
                let diags = all_diagnostics
                    .iter()
                    .find(|(f, _)| f == &g.original)
                    .map(|(_, d)| d.clone())
                    .unwrap_or_default();
                JsonFileResult {
                    file: g.original.clone(),
                    virtual_ts: g.virtual_ts.clone(),
                    diagnostics: diags,
                }
            })
            .collect();
        files.sort_by(|a, b| a.file.cmp(&b.file));
        let json_output = JsonOutput {
            files,
            error_count: total_errors,
            file_count: generated.len(),
        };
        println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
        return;
    }

    // Print diagnostics
    if !args.quiet {
        for (filename, diags) in &all_diagnostics {
            println!("\n\x1b[4m{}\x1b[0m", filename);
            for diag in diags {
                let color = if diag.starts_with("error") {
                    "\x1b[31m"
                } else {
                    "\x1b[33m"
                };
                println!("  {}{}\x1b[0m", color, diag);
            }
        }
    }

    // Print summary
    let status = if total_errors > 0 {
        "\x1b[31m\u{2717}\x1b[0m"
    } else {
        "\x1b[32m\u{2713}\x1b[0m"
    };

    println!(
        "\n{} Type checked {} files in {:.2?} (collect: {:.2?}, gen: {:.2?}, lsp: {:.2?})",
        status,
        generated.len(),
        total_time,
        collect_time,
        gen_time,
        check_time
    );

    if total_errors > 0 {
        println!("  \x1b[31m{} error(s)\x1b[0m", total_errors);
    } else {
        println!("  \x1b[32mNo type errors found!\x1b[0m");
    }

    // Profile mode: write timing report
    if args.profile {
        let profile_dir = PathBuf::from("node_modules/.vize/check-profile");
        let timing_report = serde_json::json!({
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            "files": generated.len(),
            "errors": total_errors,
            "timing": {
                "total_ms": total_time.as_secs_f64() * 1000.0,
                "gen_ms": gen_time.as_secs_f64() * 1000.0,
                "lsp_ms": check_time.as_secs_f64() * 1000.0,
            },
            "diagnostics": all_diagnostics.iter().map(|(file, diags)| {
                serde_json::json!({
                    "file": file,
                    "count": diags.len(),
                    "messages": diags,
                })
            }).collect::<Vec<_>>(),
        });
        let report_path = profile_dir.join("report.json");
        if let Err(e) = fs::write(
            &report_path,
            serde_json::to_string_pretty(&timing_report).unwrap(),
        ) {
            eprintln!("Failed to write timing report: {}", e);
        } else {
            eprintln!(
                "\x1b[33mProfile:\x1b[0m Timing report written to {}",
                report_path.display()
            );
        }
    }

    if total_errors > 0 {
        std::process::exit(1);
    }
}

/// Collect .vue files from patterns.
#[allow(clippy::disallowed_types)]
pub(crate) fn collect_vue_files(patterns: &[std::string::String]) -> Vec<PathBuf> {
    patterns
        .iter()
        .flat_map(|pattern| {
            // Extract base directory from pattern (everything before first *)
            let base_dir = if let Some(star_idx) = pattern.find('*') {
                let prefix = &pattern[..star_idx];
                // Find the last path separator before the star
                if let Some(sep_idx) = prefix.rfind('/') {
                    &pattern[..sep_idx]
                } else {
                    "."
                }
            } else {
                pattern.as_str()
            };

            // Use ignore crate's WalkBuilder for fast parallel walking (respects .gitignore)
            let walker = WalkBuilder::new(base_dir)
                .standard_filters(true) // Respect .gitignore
                .hidden(true) // Skip hidden files/dirs
                .build_parallel();

            let files: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

            walker.run(|| {
                let files = &files;
                Box::new(move |result| {
                    if let Ok(entry) = result {
                        let path = entry.path();
                        if path.extension().is_some_and(|ext| ext == "vue") {
                            if let Ok(mut f) = files.lock() {
                                f.push(path.to_path_buf());
                            }
                        }
                    }
                    ignore::WalkState::Continue
                })
            });

            files.into_inner().unwrap()
        })
        .collect()
}

/// Parse a `.d.ts` file containing `ComponentCustomProperties` augmentation.
///
/// Extracts `name: type` members from an `interface ComponentCustomProperties { ... }` block.
/// Handles multi-line types via delimiter balancing (`()`, `<>`, `{}`).
fn parse_dts_globals(
    path: &std::path::Path,
) -> Result<Vec<vize_canon::virtual_ts::TemplateGlobal>, std::io::Error> {
    use vize_canon::virtual_ts::TemplateGlobal;

    let content = fs::read_to_string(path)?;
    let mut globals = Vec::new();

    // Find the ComponentCustomProperties interface block
    let lines = content.lines();
    let mut in_interface = false;
    let mut brace_depth: i32 = 0;
    let mut current_name: Option<vize_carton::String> = None;
    let mut current_type = vize_carton::String::default();

    for line in lines {
        let trimmed = line.trim();

        if !in_interface {
            if trimmed.contains("interface ComponentCustomProperties") {
                in_interface = true;
                // Account for opening brace on same line or next
                for ch in trimmed.chars() {
                    if ch == '{' {
                        brace_depth += 1;
                    }
                }
            }
            continue;
        }

        // Inside the interface — track brace depth
        for ch in trimmed.chars() {
            if ch == '{' {
                brace_depth += 1;
            } else if ch == '}' {
                brace_depth -= 1;
            }
        }

        // Interface closed
        if brace_depth <= 0 {
            // Flush any pending member
            if let Some(name) = current_name.take() {
                let type_ann = current_type
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                globals.push(TemplateGlobal {
                    name,
                    type_annotation: type_ann.into(),
                    default_value: "{} as any".into(),
                });
            }
            break;
        }

        // Skip empty lines or brace-only lines
        if trimmed.is_empty() || trimmed == "{" || trimmed == "}" {
            continue;
        }

        // Inside the interface body (brace_depth >= 1)
        // Either continuing a multi-line type or starting a new member
        if current_name.is_some() {
            // Continuing multi-line type — append
            current_type.push(' ');
            current_type.push_str(trimmed.trim_end_matches(';'));

            if is_type_complete(&current_type) {
                let name = current_name.take().unwrap();
                let type_ann = current_type
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                globals.push(TemplateGlobal {
                    name,
                    type_annotation: type_ann.into(),
                    default_value: "{} as any".into(),
                });
                current_type = vize_carton::String::default();
            }
            continue;
        }

        // New member: "name: type" or "name?: type"
        if let Some((name_part, type_part)) = trimmed.split_once(':') {
            let name = name_part.trim().trim_end_matches('?').trim();
            if name.is_empty() {
                continue;
            }
            let type_str = type_part.trim().trim_end_matches(';');

            if is_type_complete(type_str) {
                let type_ann = type_str.split_whitespace().collect::<Vec<_>>().join(" ");
                globals.push(TemplateGlobal {
                    name: name.into(),
                    type_annotation: type_ann.into(),
                    default_value: "{} as any".into(),
                });
            } else {
                current_name = Some(name.into());
                current_type = vize_carton::String::from(type_str);
            }
        }
    }

    Ok(globals)
}

/// Check if a type annotation string has balanced delimiters.
fn is_type_complete(s: &str) -> bool {
    let mut paren = 0i32;
    let mut angle = 0i32;
    let mut brace = 0i32;
    for ch in s.chars() {
        match ch {
            '(' => paren += 1,
            ')' => paren -= 1,
            '<' => angle += 1,
            '>' => angle -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            _ => {}
        }
    }
    paren <= 0 && angle <= 0 && brace <= 0
}

/// Detect Nuxt project and add auto-import stubs to VirtualTsOptions.
/// Checks for `nuxt.config.ts` or `nuxt.config.js` in the current directory.
fn detect_nuxt_auto_imports(options: &mut vize_canon::virtual_ts::VirtualTsOptions) {
    let is_nuxt = std::path::Path::new("nuxt.config.ts").exists()
        || std::path::Path::new("nuxt.config.js").exists()
        || std::path::Path::new("nuxt.config.mts").exists();

    if !is_nuxt {
        return;
    }

    let stubs = &mut options.auto_import_stubs;

    // Vue core composables (with type-preserving signatures)
    stubs.push("declare function ref<T>(value: T): $Vue['Ref']<$Vue['UnwrapRef']<T>>;".into());
    stubs.push("declare function ref<T = any>(): $Vue['Ref']<T | undefined>;".into());
    stubs.push("declare function computed<T>(getter: () => T): $Vue['ComputedRef']<T>;".into());
    stubs.push("declare function computed<T>(options: { get: () => T; set: (value: T) => void }): $Vue['WritableComputedRef']<T>;".into());
    stubs.push(
        "declare function reactive<T extends object>(target: T): $Vue['UnwrapNestedRefs']<T>;"
            .into(),
    );
    stubs.push("declare function readonly<T extends object>(target: T): Readonly<T>;".into());
    stubs.push(
        "declare function watch(source: any, cb: (...args: any[]) => any, options?: any): any;"
            .into(),
    );
    stubs.push("declare function watchEffect(effect: () => void, options?: any): any;".into());
    stubs.push("declare function watchPostEffect(effect: () => void): any;".into());
    stubs.push("declare function watchSyncEffect(effect: () => void): any;".into());
    stubs.push("declare function onMounted(hook: () => any): void;".into());
    stubs.push("declare function onUnmounted(hook: () => any): void;".into());
    stubs.push("declare function onBeforeMount(hook: () => any): void;".into());
    stubs.push("declare function onBeforeUnmount(hook: () => any): void;".into());
    stubs.push("declare function onBeforeUpdate(hook: () => any): void;".into());
    stubs.push("declare function onUpdated(hook: () => any): void;".into());
    stubs.push("declare function onActivated(hook: () => any): void;".into());
    stubs.push("declare function onDeactivated(hook: () => any): void;".into());
    stubs.push("declare function onErrorCaptured(hook: (...args: any[]) => any): void;".into());
    stubs.push("declare function nextTick(fn?: () => void): Promise<void>;".into());
    stubs.push("declare function toRef<T extends object, K extends keyof T>(object: T, key: K): $Vue['Ref']<T[K]>;".into());
    stubs.push("declare function toRefs<T extends object>(object: T): { [K in keyof T]: $Vue['Ref']<T[K]> };".into());
    stubs.push("declare function unref<T>(ref: T | $Vue['Ref']<T>): T;".into());
    stubs.push("declare function isRef(value: any): value is $Vue['Ref'];".into());
    stubs.push("declare function shallowRef<T>(value: T): $Vue['ShallowRef']<T>;".into());
    stubs.push("declare function triggerRef(ref: $Vue['ShallowRef']): void;".into());
    stubs.push("declare function provide<T>(key: string | symbol, value: T): void;".into());
    stubs.push("declare function inject<T>(key: string | symbol, defaultValue?: T): T;".into());
    stubs.push("declare function defineAsyncComponent(source: any): any;".into());
    stubs.push("declare function h(type: any, ...args: any[]): any;".into());
    stubs.push("declare function useAttrs(): Record<string, unknown>;".into());
    stubs.push("declare function useSlots(): Record<string, (...args: any[]) => any>;".into());
    stubs.push("declare function toRaw<T>(observed: T): T;".into());
    stubs.push("declare function markRaw<T extends object>(value: T): T;".into());
    stubs.push("declare function effectScope(detached?: boolean): any;".into());
    stubs.push("declare function getCurrentScope(): any;".into());
    stubs.push("declare function onScopeDispose(fn: () => void): void;".into());
    stubs.push("declare function shallowReactive<T extends object>(target: T): T;".into());
    stubs
        .push("declare function shallowReadonly<T extends object>(target: T): Readonly<T>;".into());
    stubs.push("declare function customRef<T>(factory: any): $Vue['Ref']<T>;".into());

    // Vue Router
    stubs.push("declare function useRouter(): any;".into());
    stubs.push("declare function useRoute(): any;".into());

    // Nuxt core composables
    stubs.push("declare function definePageMeta(meta: any): void;".into());
    stubs.push("declare function useSeoMeta(meta: any): void;".into());
    stubs.push(
        "declare function useFetch<T = any>(url: string | (() => string), options?: any): any;"
            .into(),
    );
    stubs.push("declare function useAsyncData<T = any>(key: string, handler: () => Promise<T>, options?: any): any;".into());
    stubs.push(
        "declare function useLazyFetch<T = any>(url: string | (() => string), options?: any): any;"
            .into(),
    );
    stubs.push("declare function useLazyAsyncData<T = any>(key: string, handler: () => Promise<T>, options?: any): any;".into());
    stubs.push("declare function navigateTo(to: string | any, options?: any): any;".into());
    stubs.push("declare function createError(input: string | { statusCode?: number; statusMessage?: string; message?: string; data?: any; fatal?: boolean }): any;".into());
    stubs.push("declare function showError(error: any): any;".into());
    stubs.push(
        "declare function clearError(options?: { redirect?: string }): Promise<void>;".into(),
    );
    stubs.push("declare function useNuxtApp(): any;".into());
    stubs.push("declare function useRuntimeConfig(): any;".into());
    stubs.push("declare function useAppConfig(): any;".into());
    stubs.push(
        "declare function useState<T = any>(key: string, init?: () => T): $Vue['Ref']<T>;".into(),
    );
    stubs.push(
        "declare function useCookie<T = any>(name: string, options?: any): $Vue['Ref']<T>;".into(),
    );
    stubs.push("declare function useHead(input: any): void;".into());
    stubs.push(
        "declare function useRequestHeaders(headers?: string[]): Record<string, string>;".into(),
    );
    stubs.push("declare function useRequestURL(): URL;".into());
    stubs.push("declare function defineNuxtComponent(options: any): any;".into());
    stubs.push("declare function defineNuxtRouteMiddleware(middleware: any): any;".into());
    stubs.push("declare function useError(): any;".into());
    stubs.push("declare function abortNavigation(err?: any): any;".into());
    stubs.push(
        "declare function addRouteMiddleware(name: string, middleware: any, options?: any): void;"
            .into(),
    );
    stubs.push("declare function defineNuxtPlugin(plugin: any): any;".into());
    stubs.push("declare function setPageLayout(layout: string): void;".into());
    stubs.push("declare function setResponseStatus(code: number, message?: string): void;".into());
    stubs.push("declare function prerenderRoutes(routes: string | string[]): void;".into());
    stubs.push("declare function refreshNuxtData(keys?: string | string[]): Promise<void>;".into());
    stubs.push("declare function clearNuxtData(keys?: string | string[]): void;".into());
    stubs.push("declare function reloadNuxtApp(options?: any): void;".into());
    stubs.push("declare function callOnce(key: string, fn: () => any): Promise<void>;".into());
    stubs.push("declare function callOnce(fn: () => any): Promise<void>;".into());
    stubs.push("declare function onNuxtReady(callback: () => any): void;".into());
    stubs.push(
        "declare function preloadComponents(components: string | string[]): Promise<void>;".into(),
    );
    stubs.push(
        "declare function prefetchComponents(components: string | string[]): Promise<void>;".into(),
    );
    stubs.push("declare function useRequestEvent(): any;".into());
    stubs.push("declare function useRequestFetch(): typeof globalThis.fetch;".into());
    stubs
        .push("declare function useResponseHeaders(headers?: Record<string, string>): any;".into());
}
