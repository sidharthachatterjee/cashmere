mod linter;
mod lsp;

use std::path::Path;
use std::{env, fs};

use clap::Parser;
use walkdir::WalkDir;

use linter::{lint_source, LintDiagnostic};

#[derive(Parser, Debug)]
#[command(name = "cashmere")]
#[command(version)]
#[command(
    about = "A fast linter for Cloudflare Workflows TypeScript/JavaScript code, built with Rust."
)]
struct Args {
    /// Directory or file to lint (defaults to current directory)
    #[arg(default_value = ".")]
    path: String,

    /// Run as LSP server
    #[arg(long)]
    lsp: bool,
}

fn is_js_or_ts_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(
            ext,
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
        ),
        None => false,
    }
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | ".git" | "dist" | "build" | "target" | ".next" | "coverage"
    )
}

fn lint_file(path: &Path) -> Option<Vec<LintDiagnostic>> {
    let source_text = fs::read_to_string(path).ok()?;
    Some(lint_source(&source_text, path.to_str().unwrap_or("")))
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if args.lsp {
        // Run as LSP server
        lsp::run_lsp_server().await;
        return;
    }

    // Run as CLI
    let root = if args.path == "." {
        env::current_dir().expect("Failed to get current directory")
    } else {
        Path::new(&args.path).to_path_buf()
    };

    let mut all_diagnostics: Vec<LintDiagnostic> = Vec::new();
    let mut files_checked = 0;

    if root.is_file() {
        if is_js_or_ts_file(&root) {
            if let Some(diagnostics) = lint_file(&root) {
                all_diagnostics.extend(diagnostics);
                files_checked += 1;
            }
        }
    } else {
        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|e| {
                if e.file_type().is_dir() {
                    !should_skip_dir(e.file_name().to_str().unwrap_or(""))
                } else {
                    true
                }
            })
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if path.is_file() && is_js_or_ts_file(path) {
                if let Some(diagnostics) = lint_file(path) {
                    all_diagnostics.extend(diagnostics);
                    files_checked += 1;
                }
            }
        }
    }

    // Print diagnostics
    for diagnostic in &all_diagnostics {
        println!(
            "{}:{}:{} - {} [{}]",
            diagnostic.file,
            diagnostic.line,
            diagnostic.column,
            diagnostic.message,
            diagnostic.rule
        );
    }

    // Print summary
    println!();
    if all_diagnostics.is_empty() {
        println!("✓ No issues found ({} files checked)", files_checked);
    } else {
        println!(
            "✗ Found {} issue(s) in {} file(s) checked",
            all_diagnostics.len(),
            files_checked
        );
        std::process::exit(1);
    }
}
