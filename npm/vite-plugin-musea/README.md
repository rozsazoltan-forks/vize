# @vizejs/vite-plugin-musea

Vite plugin for Musea - Vue component gallery and documentation.

## Installation

Install `vp` once from the [Vite+ install guide](https://viteplus.dev/guide/install), then add the package:

```bash
vp install -D @vizejs/vite-plugin-musea
```

## Usage

```ts
// vite.config.ts
import { defineConfig } from "vite";
import { musea } from "@vizejs/vite-plugin-musea";

export default defineConfig({
  plugins: [
    musea({
      // Art files pattern
      include: "**/*.art.vue",
      // Output directory
      outDir: ".musea",
    }),
  ],
});
```

## Art File Format

```vue
<!-- Button.art.vue -->
<art title="Button" component="./Button.vue">
  <variant name="Primary" default>
    <Button variant="primary">Click me</Button>
  </variant>
  <variant name="Disabled">
    <Button disabled>Disabled</Button>
  </variant>
</art>
```

## Commands

```bash
# Start dev server
vp dev

# Build gallery
vp build
```

## License

MIT
