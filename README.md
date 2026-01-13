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

```bash
# Lint current directory
cashmere

# Lint a specific directory
cashmere ./src

# Lint a specific file
cashmere ./src/workflow.ts
```

## Supported file types

- `.js`, `.jsx`
- `.ts`, `.tsx`
- `.mjs`, `.cjs`
- `.mts`, `.cts`

## Skipped directories

The linter automatically skips: `node_modules`, `.git`, `dist`, `build`, `target`, `.next`, `coverage`
