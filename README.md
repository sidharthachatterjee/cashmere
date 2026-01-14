# cashmere

A fast linter for Cloudflare Workflows TypeScript/JavaScript code, built with Rust.

## What it does

Detects unawaited `step.do()` and `step.sleep()` calls in Cloudflare Workflows code. Not awaiting these methods creates dangling Promises that can cause race conditions and swallowed errors.

```typescript
// Bad - will be flagged
step.do('task', async () => { ... });

// Good
await step.do('task', async () => { ... });
```

## Installation

```bash
curl -fsSL https://github.com/sidharthachatterjee/cashmere/releases/latest/download/install.sh | bash
```

### Build from source

```bash
cargo build --release
```

The binary will be at `target/release/cashmere`.

## Usage

### CLI Mode

```bash
# Lint current directory
cashmere

# Lint a specific directory
cashmere ./src

# Lint a specific file
cashmere ./src/workflow.ts
```

### LSP Server Mode

Run cashmere as a Language Server Protocol (LSP) server for real-time linting in your editor:

```bash
cashmere --lsp
```

#### Editor Integration

**VS Code**

Add to your `.vscode/settings.json`:

```json
{
  "cashmere.enable": true,
  "cashmere.executablePath": "/path/to/cashmere"
}
```

Or create a VS Code extension with the following client configuration:

```typescript
const serverOptions: ServerOptions = {
  command: 'cashmere',
  args: ['--lsp']
};

const clientOptions: LanguageClientOptions = {
  documentSelector: [
    { scheme: 'file', language: 'typescript' },
    { scheme: 'file', language: 'javascript' }
  ]
};
```

**Neovim**

Add to your Neovim configuration:

```lua
vim.api.nvim_create_autocmd('FileType', {
  pattern = { 'typescript', 'javascript', 'typescriptreact', 'javascriptreact' },
  callback = function()
    vim.lsp.start({
      name = 'cashmere',
      cmd = { 'cashmere', '--lsp' },
      root_dir = vim.fs.dirname(vim.fs.find({ 'package.json' }, { upward = true })[1]),
    })
  end,
})
```

**Other Editors**

Any editor that supports LSP can be configured to use cashmere. The server communicates via stdin/stdout following the LSP specification.

## Supported file types

- `.js`, `.jsx`
- `.ts`, `.tsx`
- `.mjs`, `.cjs`
- `.mts`, `.cts`

## Skipped directories

The linter automatically skips: `node_modules`, `.git`, `dist`, `build`, `target`, `.next`, `coverage`
