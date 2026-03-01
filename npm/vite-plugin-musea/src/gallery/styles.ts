/**
 * CSS theme variables and style definitions for the Musea gallery.
 *
 * CSS is split into separate .css files and imported as text
 * via tsdown's `loader: { '.css': 'text' }` configuration.
 */

// @ts-expect-error -- CSS imported as text via tsdown loader
import stylesBase from "./styles-base.css";
// @ts-expect-error -- CSS imported as text via tsdown loader
import stylesLayout from "./styles-layout.css";
// @ts-expect-error -- CSS imported as text via tsdown loader
import stylesComponents from "./styles-components.css";

/**
 * Generate the full gallery CSS styles string.
 */
export function generateGalleryStyles(): string {
  return `${stylesBase}\n${stylesLayout}\n${stylesComponents}`;
}
