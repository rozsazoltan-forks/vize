// ============================================================================
// MuseaConfig
// ============================================================================

/**
 * VRT (Visual Regression Testing) configuration for Musea
 */
export interface MuseaVrtConfig {
  /**
   * Threshold for pixel comparison (0-1)
   * @default 0.1
   */
  threshold?: number;

  /**
   * Output directory for screenshots
   * @default '__musea_snapshots__'
   */
  outDir?: string;

  /**
   * Viewport sizes
   */
  viewports?: Array<{ width: number; height: number; name?: string }>;
}

/**
 * A11y configuration for Musea
 */
export interface MuseaA11yConfig {
  /**
   * Enable a11y checking
   * @default false
   */
  enabled?: boolean;

  /**
   * Axe-core rules to enable/disable
   */
  rules?: Record<string, boolean>;
}

/**
 * Autogen configuration for Musea
 */
export interface MuseaAutogenConfig {
  /**
   * Enable auto-generation of variants
   * @default false
   */
  enabled?: boolean;

  /**
   * Max variants to generate per component
   * @default 10
   */
  maxVariants?: number;
}

/**
 * Musea component gallery configuration
 */
export interface MuseaConfig {
  /**
   * Glob patterns for art files
   * @default ['**\/*.art.vue']
   */
  include?: string[];

  /**
   * Glob patterns to exclude
   * @default ['node_modules/**', 'dist/**']
   */
  exclude?: string[];

  /**
   * Base path for gallery
   * @default '/__musea__'
   */
  basePath?: string;

  /**
   * Enable Storybook compatibility
   * @default false
   */
  storybookCompat?: boolean;

  /**
   * Enable inline art detection in .vue files
   * @default false
   */
  inlineArt?: boolean;

  /**
   * VRT configuration
   */
  vrt?: MuseaVrtConfig;

  /**
   * A11y configuration
   */
  a11y?: MuseaA11yConfig;

  /**
   * Autogen configuration
   */
  autogen?: MuseaAutogenConfig;
}
