# Changelog

## [0.4.0] - LSP Server Support

### Added
- **LSP Server Mode**: Cashmere can now run as a Language Server Protocol (LSP) server
  - Start with `cashmere --lsp` command
  - Provides real-time diagnostics in your editor as you type
  - Works with VS Code, Neovim, Helix, Emacs, Sublime Text, and other LSP-compatible editors
  - Full support for all existing lint rules

### Changed
- Refactored linting logic into separate module (`src/linter.rs`) for reusability
- Added async runtime support using Tokio
- Main function now supports both CLI and LSP modes

### Technical Details
- Uses `tower-lsp` crate for LSP implementation
- Maintains document state with `DashMap` for thread-safe concurrent access
- LSP server provides:
  - `textDocument/didOpen`: Lint files when opened
  - `textDocument/didChange`: Lint files as you type
  - `textDocument/didSave`: Re-lint files on save
  - `textDocument/didClose`: Clean up document state

### Dependencies
- Added `tower-lsp` v0.20
- Added `tokio` v1 (with full features)
- Added `dashmap` v5
- Added `serde` v1
- Added `serde_json` v1

### Documentation
- Updated README.md with LSP usage instructions
- Added LSP_SETUP.md with detailed editor integration guides
- Added examples/workflow_example.ts demonstrating correct and incorrect patterns

## [0.3.0] and earlier
- CLI-only linter for Cloudflare Workflows
- Detects unawaited step.do(), step.sleep(), step.waitForEvent(), and step.sleepUntil() calls
- Tracks promises across variable assignments
- Supports Promise.all, Promise.race, Promise.allSettled, and Promise.any patterns
