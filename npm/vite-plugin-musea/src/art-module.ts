/**
 * Art module generation for Musea.
 *
 * Generates the virtual ES modules that represent parsed `.art.vue` files,
 * including variant component definitions and script setup handling.
 */

import path from "node:path";

import type { ArtFileInfo } from "./types/index.js";
import { toPascalCase } from "./utils.js";

/**
 * Extract the content of the first <script setup> block from a Vue SFC source.
 */
export function extractScriptSetupContent(source: string): string | undefined {
  const match = source.match(/<script\s+[^>]*setup[^>]*>([\s\S]*?)<\/script>/);
  return match?.[1]?.trim();
}

/**
 * Parse script setup content into imports and setup body.
 * Returns the import lines, setup body lines, and all identifiers to expose.
 */
export function parseScriptSetupForArt(content: string): {
  imports: string[];
  setupBody: string[];
  returnNames: string[];
} {
  const lines = content.split("\n");
  const imports: string[] = [];
  const setupBody: string[] = [];
  const returnNames: Set<string> = new Set();

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("//")) continue;

    if (trimmed.startsWith("import ")) {
      imports.push(line);
      // Extract imported names for the return statement
      const defaultMatch = trimmed.match(/^import\s+(\w+)/);
      if (defaultMatch && defaultMatch[1] !== "type") {
        returnNames.add(defaultMatch[1]);
      }
      const namedMatch = trimmed.match(/\{([^}]+)\}/);
      if (namedMatch) {
        for (const part of namedMatch[1].split(",")) {
          const name = part
            .trim()
            .split(/\s+as\s+/)
            .pop()
            ?.trim();
          if (name && !name.startsWith("type ")) {
            returnNames.add(name);
          }
        }
      }
    } else {
      setupBody.push(line);
      // Extract declared variable names
      const constMatch = trimmed.match(/^(?:const|let|var)\s+(\w+)/);
      if (constMatch) {
        returnNames.add(constMatch[1]);
      }
      // Handle destructuring: const { a, b } = ...
      const destructMatch = trimmed.match(/^(?:const|let|var)\s+\{([^}]+)\}/);
      if (destructMatch) {
        for (const part of destructMatch[1].split(",")) {
          const name = part
            .trim()
            .split(/\s*:\s*/)
            .shift()
            ?.trim();
          if (name) returnNames.add(name);
        }
      }
      // Handle array destructuring: const [a, b] = ...
      const arrayMatch = trimmed.match(/^(?:const|let|var)\s+\[([^\]]+)\]/);
      if (arrayMatch) {
        for (const part of arrayMatch[1].split(",")) {
          const name = part.trim();
          if (name && name !== "...") returnNames.add(name);
        }
      }
    }
  }

  // Remove 'type' keyword imports and common Vue utilities that shouldn't be returned
  returnNames.delete("type");

  return {
    imports,
    setupBody,
    returnNames: [...returnNames],
  };
}

export function generateArtModule(art: ArtFileInfo, filePath: string): string {
  let componentImportPath: string | undefined;
  let componentName: string | undefined;

  if (art.isInline && art.componentPath) {
    // Inline art: import the host .vue file itself as the component
    componentImportPath = art.componentPath;
    componentName = path.basename(art.componentPath, ".vue");
  } else if (art.metadata.component) {
    // Traditional .art.vue: resolve component from the component attribute
    const comp = art.metadata.component;
    componentImportPath = path.isAbsolute(comp) ? comp : path.resolve(path.dirname(filePath), comp);
    componentName = path.basename(comp, ".vue");
  }

  // Parse script setup if present
  const scriptSetup = art.scriptSetupContent
    ? parseScriptSetupForArt(art.scriptSetupContent)
    : null;

  let code = `
// Auto-generated module for: ${path.basename(filePath)}
import { defineComponent, h } from 'vue';
`;

  // Add script setup imports at module level
  // Resolve relative paths to absolute since this code runs inside a virtual module
  if (scriptSetup) {
    const artDir = path.dirname(filePath);
    for (const imp of scriptSetup.imports) {
      const resolved = imp.replace(/from\s+(['"])(\.[^'"]+)\1/, (_match, quote, relPath) => {
        const absPath = path.resolve(artDir, relPath);
        return `from ${quote}${absPath}${quote}`;
      });
      code += `${resolved}\n`;
    }
  }

  if (componentImportPath && componentName) {
    // Only add component import if not already imported by script setup
    const alreadyImported = scriptSetup?.imports.some((imp) => {
      // Check against the original relative path and the resolved absolute path
      if (
        imp.includes(`from '${componentImportPath}'`) ||
        imp.includes(`from "${componentImportPath}"`)
      )
        return true;
      // Also check by component name as default import (handles relative vs absolute path mismatch)
      return new RegExp(`^import\\s+${componentName}[\\s,]`).test(imp.trim());
    });
    if (!alreadyImported) {
      code += `import ${componentName} from '${componentImportPath}';\n`;
    }
    code += `export const __component__ = ${componentName};\n`;
  }

  code += `
export const metadata = ${JSON.stringify(art.metadata)};
export const variants = ${JSON.stringify(art.variants)};
`;

  // Generate variant components
  for (const variant of art.variants) {
    const variantComponentName = toPascalCase(variant.name);

    let template = variant.template;

    // Replace <Self> with the actual component name (for inline art)
    if (componentName) {
      template = template
        .replace(/<Self/g, `<${componentName}`)
        .replace(/<\/Self>/g, `</${componentName}>`);
    }

    // Escape the template for use in a JS string
    const escapedTemplate = template
      .replace(/\\/g, "\\\\")
      .replace(/`/g, "\\`")
      .replace(/\$/g, "\\$");

    // Wrap template with the variant container (no .musea-variant class -- the
    // outer mount container already carries it; duplicating causes double padding)
    const fullTemplate = `<div data-variant="${variant.name}">${escapedTemplate}</div>`;

    // Collect component names for the `components` option.
    // Runtime-compiled templates use resolveComponent() which checks the
    // `components` option, NOT setup return values.
    const componentNames = new Set<string>();
    if (componentName) componentNames.add(componentName);
    if (scriptSetup) {
      for (const name of scriptSetup.returnNames) {
        // PascalCase names starting with uppercase are likely components
        if (/^[A-Z]/.test(name)) componentNames.add(name);
      }
    }
    const components =
      componentNames.size > 0 ? `  components: { ${[...componentNames].join(", ")} },\n` : "";

    if (scriptSetup && scriptSetup.setupBody.length > 0) {
      // Generate variant with setup function from art file's <script setup>
      code += `
export const ${variantComponentName} = defineComponent({
  name: '${variantComponentName}',
${components}  setup() {
${scriptSetup.setupBody.map((l) => `    ${l}`).join("\n")}
    return { ${scriptSetup.returnNames.join(", ")} };
  },
  template: \`${fullTemplate}\`,
});
`;
    } else if (componentName) {
      code += `
export const ${variantComponentName} = {
  name: '${variantComponentName}',
${components}  template: \`${fullTemplate}\`,
};
`;
    } else {
      code += `
export const ${variantComponentName} = {
  name: '${variantComponentName}',
  template: \`${fullTemplate}\`,
};
`;
    }
  }

  // Default export
  const defaultVariant = art.variants.find((v) => v.isDefault) || art.variants[0];
  if (defaultVariant) {
    code += `
export default ${toPascalCase(defaultVariant.name)};
`;
  }

  return code;
}
