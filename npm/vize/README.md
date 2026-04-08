# Vize

High-performance Vue.js toolchain in Rust.

## Installation

```bash
# Install globally
npm install -g vize

# Or add it to a project
npm install -D vize

# Or install from GitHub
npm install -g github:ubugeeei/vize
```

If you use `@vizejs/vite-plugin` and want one shared config for both the plugin and the `vize`
CLI, install both packages in the project and put your config in `vize.config.*`.

## Usage

```bash
vize [COMMAND] [OPTIONS]
```

| Command | Description                           |
| ------- | ------------------------------------- |
| `build` | Compile Vue SFC files (default)       |
| `fmt`   | Format Vue SFC files                  |
| `lint`  | Lint Vue SFC files                    |
| `check` | Type check Vue SFC files              |
| `musea` | Start component gallery server        |
| `lsp`   | Start Language Server Protocol server |

```bash
vize --help           # Show help
vize <command> --help # Show command-specific help
```

## Config

`vize` looks for these files in the project root:

- `vize.config.ts`
- `vize.config.js`
- `vize.config.mjs`
- `vize.config.pkl`
- `vize.config.json`

TypeScript config:

```ts
import { defineConfig } from "vize";

export default defineConfig({
  linter: {
    preset: "opinionated",
  },
});
```

PKL config:

```pkl
amends "node_modules/vize/pkl/vize.pkl"

linter {
  preset = "opinionated"
}
```

JSON config with schema:

```json
{
  "$schema": "./node_modules/vize/schemas/vize.config.schema.json",
  "linter": {
    "preset": "opinionated"
  }
}
```

The npm CLI currently applies shared config to the `lint` command. You can override discovery with
`vize lint --config path/to/vize.config.ts` or skip config loading with `vize lint --no-config`.

## Examples

```bash
vize                              # Compile ./**/*.vue to ./dist
vize build src/**/*.vue -o out    # Custom input/output
vize build --ssr                  # SSR mode
vize fmt --check                  # Check formatting
vize lint --fix                   # Auto-fix lint issues
vize check --strict               # Strict type checking
```

## Alternative Installation

If npm installation fails, you can install via Cargo:

```bash
cargo install vize
```

## License

MIT
