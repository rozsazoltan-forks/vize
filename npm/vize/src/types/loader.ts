import type { ConfigEnv } from "./core.js";

// ============================================================================
// GlobalTypesConfig
// ============================================================================

/**
 * Global type declaration
 */
export interface GlobalTypeDeclaration {
  /**
   * TypeScript type string
   */
  type: string;

  /**
   * Default value
   */
  defaultValue?: string;
}

/**
 * Global types configuration
 */
export type GlobalTypesConfig = Record<string, GlobalTypeDeclaration | string>;

// ============================================================================
// LoadConfigOptions
// ============================================================================

/**
 * Options for loading vize.config file
 */
export interface LoadConfigOptions {
  /**
   * Config file search mode
   * - 'root': Search only in the specified root directory
   * - 'auto': Search from cwd upward until finding a config file
   * - 'none': Don't load config file
   * @default 'root'
   */
  mode?: "root" | "auto" | "none";

  /**
   * Custom config file path (overrides automatic search)
   */
  configFile?: string;

  /**
   * Config environment for dynamic config resolution
   */
  env?: ConfigEnv;
}
