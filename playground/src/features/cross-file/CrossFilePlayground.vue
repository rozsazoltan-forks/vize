<script setup lang="ts">
import "./CrossFilePlayground.css";
import { ref, computed, watch, onMounted, onUnmounted, inject, type ComputedRef } from "vue";
import MonacoEditor from "../../shared/MonacoEditor.vue";
import type { Diagnostic } from "../../shared/MonacoEditor.vue";
import type { WasmModule } from "../../wasm/index";
import { getWasm } from "../../wasm/index";
import {
  mdiClose,
  mdiInformation,
  mdiCheck,
  mdiLightbulbOn,
  mdiAlert,
  mdiRefresh,
  mdiArrowRight,
  mdiPlus,
} from "@mdi/js";
import { PRESETS } from "../../shared/presets/crossfile";
import { useResizablePanel } from "./useResizablePanel";
import { useFileManagement } from "./useFileManagement";
import { useCrossFileAnalysis } from "./useCrossFileAnalysis";
import { getFileIcon, getSeverityIcon, getTypeLabel, getTypeColor } from "./displayHelpers";

const props = defineProps<{
  compiler: WasmModule | null;
}>();
const _injectedTheme = inject<ComputedRef<"dark" | "light">>("theme");
const theme = computed<"dark" | "light">(() => _injectedTheme?.value ?? "light");

// Options
const options = ref({
  provideInject: true,
  componentEmits: true,
  fallthroughAttrs: true,
  reactivityTracking: true,
  uniqueIds: true,
  serverClientBoundary: true,
});

// Composables
const {
  containerRef,
  isResizingSidebar,
  isResizingDiagnostics,
  gridStyle,
  startSidebarResize,
  startDiagnosticsResize,
} = useResizablePanel();

const {
  currentPreset,
  files,
  activeFile,
  monacoEditorRef,
  fileNames,
  croquisResults,
  crossFileIssues,
  analysisTime,
  isAnalyzing,
  selectedIssue,
  currentSource,
  editorLanguage,
  dependencyGraph,
  addFile,
  removeFile,
  resetProject,
  selectPreset,
  selectIssue,
  selectFile,
} = useFileManagement();

const { analyzeAll } = useCrossFileAnalysis({
  files,
  croquisResults,
  crossFileIssues,
  analysisTime,
  isAnalyzing,
  dependencyGraph,
  options,
});

// Computed display properties
const currentDiagnostics = computed((): Diagnostic[] => {
  return crossFileIssues.value
    .filter((issue) => issue.file === activeFile.value)
    .map((issue) => ({
      message: `[${issue.code}] ${issue.message}${issue.suggestion ? `\n\nTip: ${issue.suggestion}` : ""}`,
      startLine: issue.line,
      startColumn: issue.column,
      endLine: issue.endLine,
      endColumn: issue.endColumn,
      severity: issue.severity,
    }));
});

const issuesByFile = computed(() => {
  const grouped: Record<string, typeof crossFileIssues.value> = {};
  for (const issue of crossFileIssues.value) {
    if (!grouped[issue.file]) grouped[issue.file] = [];
    grouped[issue.file].push(issue);
  }
  return grouped;
});

const issuesByType = computed(() => {
  const grouped: Record<string, typeof crossFileIssues.value> = {};
  for (const issue of crossFileIssues.value) {
    if (!grouped[issue.type]) grouped[issue.type] = [];
    grouped[issue.type].push(issue);
  }
  return grouped;
});

const stats = computed(() => ({
  files: Object.keys(files.value).length,
  totalIssues: crossFileIssues.value.length,
  errors: crossFileIssues.value.filter((i) => i.severity === "error").length,
  warnings: crossFileIssues.value.filter((i) => i.severity === "warning").length,
  infos: crossFileIssues.value.filter((i) => i.severity === "info").length,
}));

// Debounced auto-analysis
let analyzeTimeout: ReturnType<typeof setTimeout> | null = null;

function debouncedAnalyze() {
  if (analyzeTimeout) clearTimeout(analyzeTimeout);
  analyzeTimeout = setTimeout(() => {
    analyzeAll();
  }, 300);
}

watch(
  [files, options],
  () => {
    debouncedAnalyze();
  },
  { deep: true },
);

// Workaround for vite-plugin-vize: v-for scoped variables are not correctly
// passed to event handlers. We read the filename from DOM instead.
function setEditorValue(source: string) {
  const el = document.querySelector(".editor-content .monaco-container") as any;
  if (el?.__monacoEditor) {
    el.__monacoEditor.setValue(source);
  }
}

function handleFileClick(event: Event) {
  const el = event.currentTarget as HTMLElement;
  const name =
    el.querySelector(".tab-name, .file-name, .dep-target")?.textContent?.trim() ||
    el.textContent?.trim() ||
    "";
  if (name && files.value[name]) {
    selectFile(name);
    setEditorValue(files.value[name]);
  }
}

function handleSelectIssue(issue: (typeof crossFileIssues.value)[0]) {
  selectIssue(issue);
  setEditorValue(files.value[issue.file] ?? "");
}

// Workaround for vite-plugin-vize: v-for scoped variables are not correctly
// passed to event handlers. We read the preset name from DOM instead.
function handleSelectPreset(event: Event) {
  const el = event.currentTarget as HTMLElement;
  const name = el.querySelector(".preset-name")?.textContent?.trim();
  const preset = PRESETS.find((p) => p.name === name);
  if (preset) {
    selectPreset(preset.id, analyzeAll);
    setEditorValue(currentSource.value);
  }
}

// Workaround for vite-plugin-vize prop reactivity issue
// Use getWasm() directly with polling instead of props.compiler
let hasCompilerInitialized = false;
let pollInterval: ReturnType<typeof setInterval> | null = null;

function tryInitialize() {
  const compiler = getWasm();
  if (compiler && !hasCompilerInitialized) {
    hasCompilerInitialized = true;
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
    analyzeAll();
  }
}

onMounted(() => {
  tryInitialize();
  if (!hasCompilerInitialized) {
    pollInterval = setInterval(tryInitialize, 100);
    setTimeout(() => {
      if (pollInterval) {
        clearInterval(pollInterval);
        pollInterval = null;
      }
    }, 10000);
  }
});

onUnmounted(() => {
  if (pollInterval) {
    clearInterval(pollInterval);
    pollInterval = null;
  }
});
</script>

<template>
  <div
    ref="containerRef"
    class="cross-file-playground"
    :style="gridStyle"
    :class="{ resizing: isResizingSidebar || isResizingDiagnostics }"
  >
    <!-- Sidebar: File Tree & Dependency Graph -->
    <aside class="sidebar" aria-label="File tree and dependency graph">
      <!-- Preset Selector -->
      <div class="sidebar-section preset-section">
        <div class="section-header">
          <h3>Presets</h3>
        </div>
        <div class="preset-list">
          <button
            v-for="preset in PRESETS"
            :key="preset.id"
            :class="['preset-item', { active: currentPreset === preset.id }]"
            :title="preset.description"
            @click="handleSelectPreset($event)"
          >
            <svg class="preset-icon" viewBox="0 0 24 24">
              <path :d="preset.icon" fill="currentColor" />
            </svg>
            <span class="preset-name">{{ preset.name }}</span>
          </button>
        </div>
      </div>

      <div class="sidebar-section">
        <div class="section-header">
          <h3>Project Files</h3>
          <div class="section-actions">
            <button class="icon-btn" title="Add file" @click="addFile">
              <svg viewBox="0 0 24 24"><path :d="mdiPlus" fill="currentColor" /></svg>
            </button>
            <button class="icon-btn" title="Reset" @click="resetProject">
              <svg viewBox="0 0 24 24"><path :d="mdiRefresh" fill="currentColor" /></svg>
            </button>
          </div>
        </div>
        <nav class="file-tree">
          <div
            v-for="name in fileNames"
            :key="name"
            role="button"
            tabindex="0"
            :class="[
              'file-item',
              {
                active: activeFile === name,
                'has-errors': issuesByFile[name]?.some((i) => i.severity === 'error'),
                'has-warnings': issuesByFile[name]?.some((i) => i.severity === 'warning'),
              },
            ]"
            @click="handleFileClick($event)"
            @keydown.enter="handleFileClick($event)"
          >
            <svg class="file-icon" viewBox="0 0 24 24">
              <path :d="getFileIcon(name)" fill="currentColor" />
            </svg>
            <span class="file-name">{{ name }}</span>
            <span
              v-if="issuesByFile[name]?.length"
              class="file-badge"
              :class="issuesByFile[name].some((i) => i.severity === 'error') ? 'error' : 'warning'"
            >
              <span class="badge-count">{{ issuesByFile[name].length }}</span>
            </span>
            <button v-if="fileNames.length > 1" class="file-delete" @click.stop="removeFile(name)">
              <svg viewBox="0 0 24 24"><path :d="mdiClose" fill="currentColor" /></svg>
            </button>
          </div>
        </nav>
      </div>

      <div class="sidebar-section">
        <div class="section-header">
          <h3>Dependencies</h3>
        </div>
        <div class="dependency-graph">
          <div v-for="(deps, file) in dependencyGraph" :key="file" class="dep-node">
            <span class="dep-file">{{ file }}</span>
            <div v-if="deps.length" class="dep-arrows">
              <div v-for="dep in deps" :key="dep" class="dep-edge">
                <svg class="dep-arrow" viewBox="0 0 24 24">
                  <path :d="mdiArrowRight" fill="currentColor" />
                </svg>
                <span
                  role="button"
                  tabindex="0"
                  class="dep-target"
                  @click="handleFileClick($event)"
                  @keydown.enter="handleFileClick($event)"
                  >{{ dep }}</span
                >
              </div>
            </div>
          </div>
        </div>
      </div>

      <div class="sidebar-section options-section">
        <div class="section-header">
          <h3>Analyzers</h3>
          <span
            class="analysis-mode-badge"
            title="Strict Static Analysis: No heuristics, all issues are based on precise AST analysis"
            >STRICT</span
          >
        </div>
        <div class="options-grid">
          <label class="option-toggle">
            <input v-model="options.provideInject" type="checkbox" />
            <span>Provide/Inject</span>
          </label>
          <label class="option-toggle">
            <input v-model="options.componentEmits" type="checkbox" />
            <span>Component Emits</span>
          </label>
          <label class="option-toggle">
            <input v-model="options.fallthroughAttrs" type="checkbox" />
            <span>Fallthrough Attrs</span>
          </label>
          <label class="option-toggle">
            <input v-model="options.reactivityTracking" type="checkbox" />
            <span>Reactivity</span>
          </label>
          <label class="option-toggle">
            <input v-model="options.uniqueIds" type="checkbox" />
            <span>Unique IDs</span>
          </label>
          <label class="option-toggle">
            <input v-model="options.serverClientBoundary" type="checkbox" />
            <span>SSR Boundary</span>
          </label>
        </div>
      </div>
    </aside>

    <!-- Resize Handle: Sidebar -->
    <div
      role="button"
      tabindex="0"
      class="resize-handle resize-handle-left"
      @mousedown="startSidebarResize"
      @keydown.enter="startSidebarResize"
    ></div>

    <!-- Main Content: Editor -->
    <main class="editor-pane">
      <div class="editor-header">
        <div class="editor-tabs">
          <button
            v-for="name in fileNames"
            :key="name"
            :class="['editor-tab', { active: activeFile === name }]"
            @click="handleFileClick($event)"
          >
            <svg class="tab-icon" viewBox="0 0 24 24">
              <path :d="getFileIcon(name)" fill="currentColor" />
            </svg>
            <span class="tab-name">{{ name }}</span>
            <span
              v-if="issuesByFile[name]?.length"
              class="tab-badge"
              :class="issuesByFile[name].some((i) => i.severity === 'error') ? 'error' : 'warning'"
            >
              <span class="badge-count">{{ issuesByFile[name].length }}</span>
            </span>
          </button>
        </div>
        <div class="editor-status">
          <span v-if="isAnalyzing" class="status-analyzing">Analyzing...</span>
          <span v-else class="status-time">{{ analysisTime.toFixed(1) }}ms</span>
        </div>
      </div>
      <div class="editor-content">
        <MonacoEditor
          ref="monacoEditorRef"
          v-model="currentSource"
          :language="editorLanguage"
          :diagnostics="currentDiagnostics"
          :theme
        />
      </div>
    </main>

    <!-- Resize Handle: Diagnostics -->
    <div
      role="button"
      tabindex="0"
      class="resize-handle resize-handle-right"
      @mousedown="startDiagnosticsResize"
      @keydown.enter="startDiagnosticsResize"
    ></div>

    <!-- Right Panel: Diagnostics -->
    <aside class="diagnostics-pane" aria-label="Diagnostics">
      <div class="diagnostics-header">
        <h3>Diagnostics</h3>
        <div class="diagnostics-stats">
          <span v-if="stats.errors" class="stat-chip error">{{ stats.errors }} errors</span>
          <span v-if="stats.warnings" class="stat-chip warning">{{ stats.warnings }} warnings</span>
          <span v-if="stats.infos" class="stat-chip info">{{ stats.infos }} info</span>
        </div>
      </div>

      <div v-if="crossFileIssues.length === 0" class="diagnostics-empty">
        <svg class="empty-icon" viewBox="0 0 24 24"><path :d="mdiCheck" fill="currentColor" /></svg>
        <span>No issues detected</span>
      </div>

      <div v-else class="diagnostics-list">
        <!-- Group by type -->
        <div v-for="(issues, type) in issuesByType" :key="type" class="issue-group">
          <div class="group-header" :style="{ '--type-color': getTypeColor(type) }">
            <span class="group-badge">{{ getTypeLabel(type) }}</span>
            <span class="group-count">{{ issues.length }}</span>
          </div>
          <div class="group-issues">
            <div
              v-for="issue in issues"
              :key="issue.id"
              role="button"
              tabindex="0"
              :class="['issue-card', issue.severity, { selected: selectedIssue?.id === issue.id }]"
              @click="handleSelectIssue(issue)"
              @keydown.enter="handleSelectIssue(issue)"
            >
              <div class="issue-header">
                <svg class="severity-icon" viewBox="0 0 24 24">
                  <path :d="getSeverityIcon(issue.severity)" fill="currentColor" />
                </svg>
                <span class="issue-code">{{ issue.code }}</span>
                <span class="issue-location">{{ issue.file }}:{{ issue.line }}</span>
              </div>
              <div class="issue-message">{{ issue.message }}</div>
              <div v-if="issue.suggestion" class="issue-suggestion">
                <svg class="suggestion-icon" viewBox="0 0 24 24">
                  <path :d="mdiArrowRight" fill="currentColor" />
                </svg>
                <span class="suggestion-text">{{ issue.suggestion }}</span>
              </div>
              <div v-if="issue.relatedLocations?.length" class="issue-related">
                <div v-for="(rel, i) in issue.relatedLocations" :key="i" class="related-item">
                  <span class="related-loc">{{ rel.file }}:{{ rel.line }}</span>
                  <span class="related-msg">{{ rel.message }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </aside>
  </div>
</template>
