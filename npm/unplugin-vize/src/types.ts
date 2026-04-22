export interface SfcCompileOptionsNapi {
  filename?: string;
  sourceMap?: boolean;
  ssr?: boolean;
  vapor?: boolean;
  customRenderer?: boolean;
  scopeId?: string;
}

export interface SfcCompileResultNapi {
  code: string;
  css?: string;
  errors: string[];
  warnings: string[];
  templateHash?: string;
  styleHash?: string;
  scriptHash?: string;
}

export interface VizeUnpluginOptions {
  include?: string | RegExp | Array<string | RegExp>;
  exclude?: string | RegExp | Array<string | RegExp>;
  isProduction?: boolean;
  ssr?: boolean;
  sourceMap?: boolean;
  vapor?: boolean;
  customRenderer?: boolean;
  root?: string;
  debug?: boolean;
}

export interface StyleBlockInfo {
  content: string;
  src?: string | null;
  lang: string | null;
  scoped: boolean;
  module: boolean | string;
  index: number;
}

export interface CompiledModule {
  code: string;
  css?: string;
  scopeId: string;
  hasScoped: boolean;
  templateHash?: string;
  styleHash?: string;
  scriptHash?: string;
  styles: StyleBlockInfo[];
}

export interface CachedCompiledModule {
  compiled: CompiledModule;
  sourceHash: string;
  signature: string;
}

export interface NormalizedVizeUnpluginOptions {
  include?: string | RegExp | Array<string | RegExp>;
  exclude?: string | RegExp | Array<string | RegExp>;
  isProduction: boolean;
  ssr: boolean;
  sourceMap: boolean;
  vapor: boolean;
  customRenderer: boolean;
  root: string;
  debug: boolean;
}

export interface ParsedVueRequestQuery {
  vue: boolean;
  type: string | null;
  index: number | null;
  lang: string | null;
  module: boolean | string;
  scoped: string | null;
  vizeFile: string | null;
}

export interface ParsedVueRequest {
  filename: string;
  path: string;
  query: ParsedVueRequestQuery;
}
