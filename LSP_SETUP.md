# LSP Server Setup Guide

This guide explains how to use cashmere as a Language Server Protocol (LSP) server for real-time linting in your editor.

## Running the LSP Server

```bash
cashmere --lsp
```

The server communicates via stdin/stdout following the LSP specification.

## Editor Integration Examples

### VS Code

Create a simple VS Code extension or use a generic LSP client extension.

**Option 1: Create a Custom Extension**

1. Create a new directory for your extension:
```bash
mkdir cashmere-vscode
cd cashmere-vscode
npm init -y
npm install vscode-languageclient
```

2. Create `src/extension.ts`:
```typescript
import * as path from 'path';
import { workspace, ExtensionContext } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
    const serverOptions: ServerOptions = {
        command: 'cashmere', // Ensure cashmere is in PATH
        args: ['--lsp'],
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'typescript' },
            { scheme: 'file', language: 'javascript' },
            { scheme: 'file', language: 'typescriptreact' },
            { scheme: 'file', language: 'javascriptreact' },
        ],
    };

    client = new LanguageClient(
        'cashmere',
        'Cashmere Linter',
        serverOptions,
        clientOptions
    );

    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
```

3. Create `package.json`:
```json
{
    "name": "cashmere-vscode",
    "displayName": "Cashmere Linter",
    "description": "Cloudflare Workflows linter",
    "version": "0.1.0",
    "engines": {
        "vscode": "^1.75.0"
    },
    "categories": ["Linters"],
    "activationEvents": [
        "onLanguage:typescript",
        "onLanguage:javascript"
    ],
    "main": "./out/extension.js",
    "contributes": {
        "configuration": {
            "type": "object",
            "title": "Cashmere",
            "properties": {
                "cashmere.enable": {
                    "type": "boolean",
                    "default": true,
                    "description": "Enable Cashmere linter"
                }
            }
        }
    }
}
```

**Option 2: Use Generic LSP Client**

Install the [vscode-languageclient](https://marketplace.visualstudio.com/items?itemName=chris-hayes.vscode-languageclient) extension and configure it in `.vscode/settings.json`:

```json
{
    "languageServerExample.trace.server": "verbose",
    "cashmere.lsp.command": "cashmere",
    "cashmere.lsp.args": ["--lsp"]
}
```

### Neovim (nvim-lspconfig)

Add to your Neovim configuration (`init.lua` or `lspconfig.lua`):

```lua
-- Add this to your LSP configuration
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

-- Check if cashmere config already exists
if not configs.cashmere then
  configs.cashmere = {
    default_config = {
      cmd = { 'cashmere', '--lsp' },
      filetypes = { 'typescript', 'javascript', 'typescriptreact', 'javascriptreact' },
      root_dir = function(fname)
        return lspconfig.util.find_git_ancestor(fname) or vim.fn.getcwd()
      end,
      settings = {},
    },
  }
end

-- Start cashmere LSP server
lspconfig.cashmere.setup({
  on_attach = function(client, bufnr)
    -- Your on_attach configuration
    print("Cashmere LSP attached")
  end,
  capabilities = require('cmp_nvim_lsp').default_capabilities(),
})
```

Or use a simpler autocmd approach:

```lua
vim.api.nvim_create_autocmd('FileType', {
  pattern = { 'typescript', 'javascript', 'typescriptreact', 'javascriptreact' },
  callback = function()
    vim.lsp.start({
      name = 'cashmere',
      cmd = { 'cashmere', '--lsp' },
      root_dir = vim.fs.dirname(vim.fs.find({ 'package.json', '.git' }, { upward = true })[1]),
    })
  end,
})
```

### Helix

Add to your `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "typescript"
language-servers = ["typescript-language-server", "cashmere"]

[[language]]
name = "javascript"
language-servers = ["typescript-language-server", "cashmere"]

[language-server.cashmere]
command = "cashmere"
args = ["--lsp"]
```

### Emacs (lsp-mode)

Add to your Emacs configuration:

```elisp
(require 'lsp-mode)

(add-to-list 'lsp-language-id-configuration '(typescript-mode . "typescript"))
(add-to-list 'lsp-language-id-configuration '(javascript-mode . "javascript"))

(lsp-register-client
 (make-lsp-client
  :new-connection (lsp-stdio-connection '("cashmere" "--lsp"))
  :activation-fn (lsp-activate-on "typescript" "javascript")
  :server-id 'cashmere))

(add-hook 'typescript-mode-hook #'lsp)
(add-hook 'javascript-mode-hook #'lsp)
```

### Sublime Text (LSP)

Install the [LSP package](https://packagecontrol.io/packages/LSP) and add to your LSP settings:

```json
{
    "clients": {
        "cashmere": {
            "enabled": true,
            "command": ["cashmere", "--lsp"],
            "selector": "source.ts | source.js | source.tsx | source.jsx"
        }
    }
}
```

## Features

The LSP server provides:

- **Real-time diagnostics**: Errors appear as you type
- **On-save linting**: Diagnostics refresh when you save files
- **Multi-file support**: Lints all open TypeScript/JavaScript files
- **Full LSP compliance**: Works with any LSP-compatible editor

## Supported File Types

- `.ts` - TypeScript
- `.tsx` - TypeScript React
- `.js` - JavaScript
- `.jsx` - JavaScript React
- `.mjs` - ES Module JavaScript
- `.cjs` - CommonJS JavaScript
- `.mts` - ES Module TypeScript
- `.cts` - CommonJS TypeScript

## Troubleshooting

### LSP server not starting

1. Verify cashmere is installed and in your PATH:
   ```bash
   which cashmere
   cashmere --version
   ```

2. Test the LSP server manually:
   ```bash
   cashmere --lsp
   ```
   The server should start and wait for input (use Ctrl+C to exit).

3. Check editor LSP logs for connection errors

### Diagnostics not appearing

1. Ensure the file type is supported (see list above)
2. Check that the file contains actual linting issues
3. Verify the LSP client is enabled in your editor settings
4. Check editor LSP logs for errors

### Performance issues

The LSP server lints files on-demand and should be very fast. If you experience slowness:

1. Check if other LSP servers or extensions are conflicting
2. Ensure you're using the release build: `cargo build --release`
3. Monitor CPU usage to identify bottlenecks
