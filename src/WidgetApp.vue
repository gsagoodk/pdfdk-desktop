<script setup lang="ts">
// A desktop widget: one tool, drop files in, drag results out. Mounted instead of App.vue
// when the window URL carries ?widget=<id> (see main.ts). Runs inside a small frameless
// always-on-bottom window with the OS's frosted-glass behind it.
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { startDrag } from "@crabnebula/tauri-plugin-drag";
import dragIcon from "./assets/drag-icon.png?inline";
import { t, bi, lang } from "./i18n";
import { CATEGORY_VAR, baseName, fileExt, type AuthState, type FileResult, type ToolDefinition, type WidgetConfig } from "./types";

const id = new URLSearchParams(location.search).get("widget") || "";

const widget = ref<WidgetConfig | null>(null);
const tool = ref<ToolDefinition | null>(null);
const auth = ref<AuthState | null>(null);
const error = ref("");
const notice = ref("");
const dragOver = ref(false);
const running = ref(false);
const runningCount = ref(0);
const progress = ref<{ done: number; total: number; file: string } | null>(null);
const results = ref<FileResult[] | null>(null);

const signedIn = computed(() => !!auth.value?.isAuthenticated);
const hasPro = computed(() => !!auth.value && auth.value.isPro);
const ready = computed(() => signedIn.value && hasPro.value && !!tool.value);
const accent = computed(() => `var(${CATEGORY_VAR[tool.value?.category || ""] || "--brand"})`);
const outputs = computed(() => (results.value || []).filter((r) => r.output).map((r) => r.output as string));
const failed = computed(() => (results.value || []).filter((r) => !r.output));

const mq = window.matchMedia("(prefers-color-scheme: dark)");
function applyTheme(theme: string) {
  const dark = theme === "dark" || (theme !== "light" && mq.matches);
  document.documentElement.setAttribute("data-theme", dark ? "dark" : "light");
}
const onScheme = () => applyTheme(widget.value ? currentTheme : "system");
let currentTheme = "system";

async function load() {
  try {
    const info = await invoke<{ widget: WidgetConfig; tool: ToolDefinition; language: string; theme: string }>("widget_get", { id });
    widget.value = info.widget;
    tool.value = info.tool;
    lang.value = info.language === "en" ? "en" : "da";
    document.documentElement.lang = lang.value;
    currentTheme = info.theme;
    applyTheme(currentTheme);
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}
async function refreshAuth() {
  try { auth.value = await invoke<AuthState>("get_auth_state"); } catch { auth.value = null; }
}
// Widgets open before the main window has validated the saved session. Ask the app to check
// the saved token ourselves, and keep asking for a while in case the network is slow (0.4.3).
let authTries = 0;
let authTimer: ReturnType<typeof setTimeout> | null = null;
async function settleAuth() {
  await refreshAuth();
  if (signedIn.value || authTries >= 20) return;
  authTries += 1;
  if (authTries === 1) {
    try { auth.value = await invoke<AuthState>("check_auth"); } catch { /* keep polling */ }
    if (signedIn.value) return;
  }
  authTimer = setTimeout(settleAuth, 3000);
}

let flashTimer: ReturnType<typeof setTimeout> | null = null;
function flash(msg: string) {
  notice.value = msg;
  if (flashTimer) clearTimeout(flashTimer);
  flashTimer = setTimeout(() => (notice.value = ""), 2600);
}

async function handleDrop(paths: string[]) {
  if (!ready.value || running.value || !tool.value) return;
  const accepts = tool.value.accepts.map((a) => a.toLowerCase());
  const ok = paths.filter((p) => accepts.includes(fileExt(p)));
  if (!ok.length) {
    flash(t("widgetOnly", { ext: accepts.map((a) => "." + a).join(" ") }));
    return;
  }
  if (ok.length < paths.length) flash(t("widgetSkipped", { n: paths.length - ok.length }));
  running.value = true;
  runningCount.value = ok.length;
  progress.value = ok.length > 1 ? { done: 0, total: ok.length, file: baseName(ok[0]) } : null;
  results.value = null;
  try {
    results.value = await invoke<FileResult[]>("widget_process", { id, paths: ok });
  } catch (e) {
    results.value = ok.map((p) => ({ input: p, output: null, error: String(e) }));
  } finally {
    running.value = false;
    progress.value = null;
  }
}

// ---- drag a result out: mousedown + a few px of movement starts the native drag
let pressAt: { x: number; y: number; path: string } | null = null;
function pressResult(e: MouseEvent, path: string) {
  if (e.button !== 0) return;
  pressAt = { x: e.clientX, y: e.clientY, path };
}
async function moveResult(e: MouseEvent) {
  if (!pressAt) return;
  if (Math.abs(e.clientX - pressAt.x) + Math.abs(e.clientY - pressAt.y) < 6) return;
  const path = pressAt.path;
  pressAt = null;
  try {
    await startDrag({ item: [path], icon: dragIcon, mode: "copy" });
  } catch (err) {
    invoke("log_client", { message: `widget drag-out failed: ${String(err)}` }).catch(() => {});
    flash(t("widgetDragFailed"));
  }
}
function releaseResult(path: string) {
  if (pressAt && pressAt.path === path) revealItemInDir(path).catch(() => {});
  pressAt = null;
}
function dragAll() {
  if (outputs.value.length) startDrag({ item: outputs.value, icon: dragIcon, mode: "copy" }).catch(() => flash(t("widgetDragFailed")));
}

function clear() { results.value = null; }
async function remove() { await invoke("widget_remove", { id }).catch(() => {}); }
function openMain() { invoke("widget_show_main").catch(() => {}); }

let unDrag: UnlistenFn | null = null;
let unAuth: UnlistenFn | null = null;
let unChanged: UnlistenFn | null = null;
let unProgress: UnlistenFn | null = null;

onMounted(async () => {
  mq.addEventListener("change", onScheme);
  await Promise.all([load(), settleAuth()]);
  unAuth = await listen("auth-changed", refreshAuth);
  unChanged = await listen("widget-changed", load);
  unProgress = await listen<{ done: number; total: number; file: string; ok: boolean }>("widget-progress", (e) => {
    if (running.value && e.payload.total > 1) progress.value = { done: e.payload.done, total: e.payload.total, file: e.payload.file };
  });
  unDrag = await getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type === "enter" || e.payload.type === "over") dragOver.value = true;
    else if (e.payload.type === "leave") dragOver.value = false;
    else if (e.payload.type === "drop") {
      dragOver.value = false;
      if (e.payload.paths.length) handleDrop(e.payload.paths);
    }
  });
});
onBeforeUnmount(() => {
  mq.removeEventListener("change", onScheme);
  unDrag?.(); unAuth?.(); unChanged?.(); unProgress?.();
  if (authTimer) clearTimeout(authTimer);
});
</script>

<template>
  <div class="w" :class="{ over: dragOver, busy: running }" :style="{ '--acc': accent }" @mousemove="moveResult">
    <!-- header: move handle -->
    <div class="w-head" data-tauri-drag-region>
      <span class="w-dot" data-tauri-drag-region></span>
      <span class="w-name" data-tauri-drag-region>{{ tool ? bi(tool.name) : "PDF.dk" }}</span>
      <button class="w-x" :title="t('widgetRemove')" @click="remove">×</button>
    </div>

    <!-- states -->
    <div v-if="error" class="w-body w-msg">
      <p>{{ error }}</p>
    </div>
    <div v-else-if="!signedIn || !hasPro" class="w-body w-msg">
      <p>{{ signedIn ? t("widgetNeedsPro") : t("widgetSignIn") }}</p>
      <button class="w-btn" @click="openMain">{{ t("widgetOpenApp") }}</button>
    </div>
    <div v-else-if="running" class="w-body w-msg">
      <div class="w-spin"></div>
      <template v-if="progress">
        <div class="w-bar"><span :style="{ width: Math.round((progress.done / progress.total) * 100) + '%' }"></span></div>
        <p class="w-prog">{{ progress.done }} / {{ progress.total }}</p>
        <p class="w-prog-file">{{ progress.file }}</p>
      </template>
      <p v-else>{{ t("widgetRunning", { n: runningCount }) }}</p>
    </div>
    <div v-else-if="results" class="w-body w-res">
      <div class="w-list">
        <div
          v-for="p in outputs.slice(0, 3)" :key="p" class="w-file" :title="t('widgetDragHint')"
          @mousedown="pressResult($event, p)" @mouseup="releaseResult(p)"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></svg>
          <span>{{ baseName(p) }}</span>
        </div>
        <div v-if="outputs.length > 3" class="w-more" @mousedown.prevent="dragAll">+{{ outputs.length - 3 }}</div>
        <div v-for="f in failed.slice(0, 2)" :key="f.input" class="w-file w-fail" :title="f.error || ''">
          <span>{{ baseName(f.input) }}</span>
        </div>
      </div>
      <div class="w-foot">
        <span v-if="outputs.length" class="w-hint">{{ t("widgetDragOut") }}</span>
        <span v-else class="w-hint w-hint-bad">{{ t("widgetFailed") }}</span>
        <button class="w-link" @click="clear">{{ t("widgetClear") }}</button>
      </div>
    </div>
    <div v-else class="w-body w-drop">
      <div class="w-zone">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5"/><path d="M12 15V3"/></svg>
        <p>{{ t("widgetDrop") }}</p>
      </div>
    </div>

    <transition name="w-fade"><div v-if="notice" class="w-notice">{{ notice }}</div></transition>
  </div>
</template>

<style>
/* the window itself is transparent; the OS draws the frosted glass behind us */
html, body, #app { background: transparent !important; margin: 0; height: 100%; overflow: hidden; }
</style>

<style scoped>
.w {
  position: relative; box-sizing: border-box; width: 100vw; height: 100vh; padding: 12px 12px 10px;
  display: flex; flex-direction: column; gap: 6px; border-radius: 22px; overflow: hidden;
  font-family: var(--font); color: var(--text); user-select: none; -webkit-user-select: none; cursor: default;
  background: rgba(255, 255, 255, 0.62); border: 1px solid rgba(255, 255, 255, 0.55);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.25);
  transition: box-shadow .15s, background .15s;
}
html[data-theme="dark"] .w { background: rgba(28, 28, 33, 0.58); border-color: rgba(255, 255, 255, 0.10); box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.04); }
.w.over { box-shadow: inset 0 0 0 2px var(--acc); background: color-mix(in srgb, var(--acc) 10%, rgba(255, 255, 255, 0.62)); }
html[data-theme="dark"] .w.over { background: color-mix(in srgb, var(--acc) 14%, rgba(28, 28, 33, 0.58)); }

.w-head { display: flex; align-items: center; gap: 7px; height: 22px; flex: none; cursor: grab; }
.w-head:active { cursor: grabbing; }
.w-dot { width: 9px; height: 9px; border-radius: 50%; background: var(--acc); flex: none; }
.w-name { font-size: 12.5px; font-weight: 800; letter-spacing: -.01em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; min-width: 0; }
.w-x { width: 20px; height: 20px; border: 0; border-radius: 6px; background: transparent; color: var(--muted); font-size: 15px; line-height: 1; cursor: pointer; opacity: 0; transition: opacity .12s; flex: none; }
.w:hover .w-x { opacity: 1; }
.w-x:hover { background: color-mix(in srgb, var(--text) 10%, transparent); color: var(--text); }

.w-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.w-msg { align-items: center; justify-content: center; text-align: center; gap: 8px; }
.w-msg p { margin: 0; font-size: 12px; color: var(--muted); line-height: 1.35; }
.w-btn { height: 28px; padding: 0 12px; border: 0; border-radius: 8px; background: var(--acc); color: #fff; font-weight: 700; font-size: 12px; cursor: pointer; font-family: inherit; }

.w-drop { }
.w-zone { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; border: 1.5px dashed color-mix(in srgb, var(--text) 22%, transparent); border-radius: 14px; color: var(--muted); transition: border-color .15s, color .15s; }
.w.over .w-zone { border-color: var(--acc); color: var(--acc); }
.w-zone svg { width: 26px; height: 26px; }
.w-zone p { margin: 0; font-size: 12px; font-weight: 600; }

.w-bar { width: 70%; height: 5px; border-radius: 999px; background: color-mix(in srgb, var(--text) 12%, transparent); overflow: hidden; }
.w-bar span { display: block; height: 100%; background: var(--acc); border-radius: 999px; transition: width .25s; }
.w-prog { margin: 0; font-size: 13px; font-weight: 800; color: var(--text); font-variant-numeric: tabular-nums; }
.w-prog-file { margin: 0; font-size: 10.5px; color: var(--muted); max-width: 90%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.w-spin { width: 26px; height: 26px; border-radius: 50%; border: 2.5px solid color-mix(in srgb, var(--acc) 25%, transparent); border-top-color: var(--acc); animation: w-rot .8s linear infinite; }
@keyframes w-rot { to { transform: rotate(360deg); } }

.w-res { gap: 6px; }
.w-list { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 4px; overflow: hidden; }
.w-file { display: flex; align-items: center; gap: 7px; height: 30px; padding: 0 9px; border-radius: 9px; background: color-mix(in srgb, var(--text) 7%, transparent); font-size: 11.5px; font-weight: 600; cursor: grab; }
.w-file:hover { background: color-mix(in srgb, var(--acc) 16%, transparent); }
.w-file:active { cursor: grabbing; }
.w-file svg { width: 14px; height: 14px; flex: none; color: var(--acc); }
.w-file span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.w-fail { color: var(--bad); background: color-mix(in srgb, var(--bad) 10%, transparent); cursor: default; }
.w-more { font-size: 11px; font-weight: 700; color: var(--muted); padding: 0 9px; cursor: grab; }
.w-foot { display: flex; align-items: center; justify-content: space-between; gap: 6px; height: 18px; flex: none; }
.w-hint { font-size: 10.5px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.w-hint-bad { color: var(--bad); }
.w-link { border: 0; background: transparent; color: var(--acc); font-size: 11px; font-weight: 700; cursor: pointer; padding: 0; font-family: inherit; flex: none; }

.w-notice { position: absolute; left: 12px; right: 12px; bottom: 10px; padding: 6px 9px; border-radius: 9px; background: var(--text); color: var(--bg); font-size: 11px; font-weight: 600; text-align: center; }
.w-fade-enter-active, .w-fade-leave-active { transition: opacity .18s; }
.w-fade-enter-from, .w-fade-leave-to { opacity: 0; }
</style>
