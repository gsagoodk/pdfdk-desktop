<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, watch, markRaw } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { t, lang } from "./i18n";
import { emptyAuth, type AppConfig, type AuthState, type Job, type ToolDefinition } from "./types";
import LoginView from "./components/LoginView.vue";
import ToolsView from "./components/ToolsView.vue";
import ActivityView from "./components/ActivityView.vue";
import SettingsView from "./components/SettingsView.vue";
import QuickAction from "./components/QuickAction.vue";

type View = "tools" | "activity" | "settings";

const loading = ref(true);
const auth = ref<AuthState>(emptyAuth());
const config = ref<AppConfig | null>(null);
const tools = ref<ToolDefinition[]>([]);
const toolsLoading = ref(false);
const view = ref<View>("tools");
const siteBase = ref("https://pdf.dk");
const version = ref("");
const update = ref<Update | null>(null);
const updating = ref(false);
const updateError = ref("");
const updateReady = ref(false);   // installed in the background, waiting for a restart
const droppedFiles = ref<string[]>([]);
// One Pro account, one computer (shared with PDF.dk Desktop on this machine)
interface Seat { state: "here" | "elsewhere" | "unknown"; name?: string | null; lastSeen?: string | null; nextAllowed?: string | null; message?: string | null }
const seat = ref<Seat>({ state: "unknown" });
const moving = ref(false);
async function checkSeat() {
  if (!signedIn.value) return;
  seat.value = await invoke<Seat>("seat_status").catch(() => ({ state: "unknown" }) as Seat);
}
async function moveHere() {
  moving.value = true;
  try {
    seat.value = await invoke<Seat>("seat_activate");
    if (seat.value.state === "here") await invoke("start_watchers").catch((e) => console.error(e));
  } finally {
    moving.value = false;
  }
}
const seatWhen = (s?: string | null) => {
  if (!s) return "";
  const d = new Date(s);
  return isNaN(+d) ? s : d.toLocaleString(lang.value === "en" ? "en-GB" : "da-DK", { dateStyle: "medium", timeStyle: "short" });
};
const dragOver = ref(false);

const signedIn = computed(() => auth.value.isAuthenticated);
// the app is for Pro (incl. trial, team, admin-granted) — free accounts see the gate
const hasPro = computed(() => auth.value.isPro || !!auth.value.isUnlimited || !!auth.value.user?.isSuperadmin);
const planLabel = computed(() => {
  const p = (auth.value.plan || "free").toLowerCase();
  if (p === "team" || p === "superadmin" || p === "enterprise") return "PRO";
  return p.toUpperCase();
});
const usageText = computed(() => {
  if (auth.value.isUnlimited) return t("unlimited");
  if (auth.value.jobsLimit) return `${auth.value.jobsUsed ?? 0} / ${auth.value.jobsLimit} ${t("runsThisMonth")}`;
  return "";
});

// ---- theme + language follow the config
function applyTheme(theme: string) {
  const dark = theme === "dark" || (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}
const mq = window.matchMedia("(prefers-color-scheme: dark)");
const onScheme = () => applyTheme(config.value?.general.theme || "system");
watch(() => config.value?.general.theme, (th) => applyTheme(th || "system"));
watch(() => config.value?.general.language, (l) => { lang.value = l === "en" ? "en" : "da"; document.documentElement.lang = lang.value; });

// ---- data
async function loadConfig() {
  config.value = await invoke<AppConfig>("get_config");
}
async function loadTools(refresh = false) {
  toolsLoading.value = true;
  try {
    tools.value = await invoke<ToolDefinition[]>("get_available_tools", { refresh });
  } finally {
    toolsLoading.value = false;
  }
}
/** `justSignedIn`: the user signed in now (not the app starting) – then this computer takes the
 *  account's Pro if another computer has it (one Pro, one computer). */
async function afterSignIn(justSignedIn = false) {
  await loadConfig();
  // always refetch the catalogue on start; the Rust side falls back to the cached copy when offline
  await loadTools(true);
  if (!hasPro.value) return;   // nothing runs for a free account
  await checkSeat();
  if (justSignedIn && seat.value.state === "elsewhere") seat.value = await invoke<Seat>("seat_activate");
  // Pro is on another computer: the folders wait here until it is moved (pdf.dk would refuse the jobs)
  if (seat.value.state !== "elsewhere") await invoke("start_watchers").catch((e) => console.error(e));
  const pending = await invoke<string[]>("take_pending_files");
  if (pending.length) droppedFiles.value = pending;
}
async function refreshAuth() {
  auth.value = await invoke<AuthState>("check_auth");
}
/** "Check again" on the Pro gate: the user just started a trial on the website */
async function recheckPlan() {
  await refreshAuth();
  if (hasPro.value) await afterSignIn(true);
}
async function signOut() {
  await invoke("logout");
  auth.value = emptyAuth();
  view.value = "tools";
}
async function saveConfig(next: AppConfig) {
  config.value = next;
  await invoke("save_config", { newConfig: next });
}

// ---- updates
async function checkForUpdates(): Promise<boolean> {
  try {
    const u = await check();
    // markRaw: the Update object has private class fields, which cannot be read through
    // Vue's reactive proxy — that made every install fail with "Cannot read private member"
    if (u) {
      update.value = markRaw(u);
      // silent updates (0.4.5): install by ourselves when nothing is running
      if (config.value?.general.autoUpdate !== false) autoInstall();
      return true;
    }
  } catch (e) {
    console.log("updater:", e);
  }
  return false;
}
/// Download + install without asking; restart right away when nobody is looking at the
/// window, otherwise show a "restart" button. Waits while jobs are running.
async function autoInstall() {
  if (!update.value || updating.value || updateReady.value) return;
  const jobs = await invoke<Job[]>("get_jobs").catch(() => [] as Job[]);
  if (jobs.some((j) => j.status === "processing" || j.status === "queued")) {
    setTimeout(autoInstall, 5 * 60 * 1000);
    return;
  }
  updating.value = true;
  try {
    await update.value.downloadAndInstall();
    updateReady.value = true;
    invoke("log_client", { message: `Update ${update.value.version} installed in the background` }).catch(() => {});
    if (document.visibilityState === "hidden") await relaunch();
  } catch (e) {
    const msg = String(e);
    updateError.value = msg;
    invoke("log_client", { message: `Background update to ${update.value?.version} failed: ${msg}` }).catch(() => {});
  } finally {
    updating.value = false;
  }
}
async function installUpdate() {
  if (!update.value) return;
  if (updateReady.value) { await relaunch(); return; }
  updating.value = true;
  try {
    await update.value.downloadAndInstall();
    await relaunch();
  } catch (e) {
    // say what went wrong (and keep it in the app log) instead of silently sending
    // people to the download page
    const msg = String(e);
    console.error("update failed", e);
    updateError.value = msg;
    invoke("log_client", { message: `Update to ${update.value?.version} failed: ${msg}` }).catch(() => {});
    updating.value = false;
  }
}

// ---- files from the OS: drag-drop on the window, dock drop, open-with
let unlistenOpened: UnlistenFn | null = null;
let unlistenDrag: UnlistenFn | null = null;

onMounted(async () => {
  document.documentElement.lang = "da";
  mq.addEventListener("change", onScheme);
  version.value = await getVersion();
  siteBase.value = await invoke<string>("get_site_base");
  await loadConfig();
  applyTheme(config.value?.general.theme || "system");
  lang.value = config.value?.general.language === "en" ? "en" : "da";

  await refreshAuth();
  if (signedIn.value) await afterSignIn();
  loading.value = false;
  checkForUpdates().then(async (found) => {
    if (found && (await invoke<boolean>("auto_update_test").catch(() => false))) {
      invoke("log_client", { message: `auto-update test: installing ${update.value?.version}` }).catch(() => {});
      installUpdate();
    }
  });
  // the app runs for weeks in the background: look again every 6 hours, and when the
  // window is brought back (the moment someone would notice an "Opdatér" button)
  setInterval(() => { if (!update.value) checkForUpdates(); }, 6 * 60 * 60 * 1000);
  // Pro moved to or from another computer while this one runs
  setInterval(checkSeat, 15 * 60 * 1000);
  document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible" && !update.value) checkForUpdates(); });

  unlistenOpened = await listen<string[]>("files-opened", (e) => {
    if (signedIn.value) droppedFiles.value = [...droppedFiles.value, ...e.payload];
  });
  unlistenDrag = await getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type === "enter" || e.payload.type === "over") dragOver.value = true;
    else if (e.payload.type === "leave") dragOver.value = false;
    else if (e.payload.type === "drop") {
      dragOver.value = false;
      if (signedIn.value && e.payload.paths.length) droppedFiles.value = e.payload.paths;
    }
  });
});
onBeforeUnmount(() => {
  mq.removeEventListener("change", onScheme);
  unlistenOpened?.();
  unlistenDrag?.();
});
</script>

<template>
  <div class="app" :class="{ 'drag-over': dragOver }">
    <div v-if="loading" class="boot">
      <div class="spin"></div>
    </div>

    <LoginView
      v-else-if="!signedIn"
      :site-base="siteBase"
      @signed-in="(a) => { auth = a; afterSignIn(true); }"
    />

    <div v-else-if="!hasPro" class="gate">
      <div class="gate-card card">
        <span class="pdk-brand" style="font-size:30px">pdf<i class="pdk-dot"></i>dk</span>
        <h1 class="gate-h">{{ t("proRequiredTitle") }}</h1>
        <p class="gate-p">{{ t("proRequiredBody") }}</p>
        <p class="gate-who mono">{{ auth.user?.email }}</p>
        <div class="gate-actions">
          <button class="btn btn-primary btn-lg" @click="openUrl(`${siteBase}/priser`)">{{ t("seePrices") }}</button>
          <button class="btn btn-lg" @click="recheckPlan">{{ t("checkAgain") }}</button>
          <button class="btn btn-ghost btn-lg" @click="signOut">{{ t("signOut") }}</button>
        </div>
      </div>
    </div>

    <template v-else>
      <header class="hdr">
        <div class="hdr-l">
          <span class="pdk-brand hdr-brand">pdf<i class="pdk-dot"></i>dk</span>
          <span class="hdr-sub">Flow</span>
        </div>
        <nav class="hdr-nav">
          <button class="nav-btn" :class="{ on: view === 'tools' }" @click="view = 'tools'">{{ t("tools") }}</button>
          <button class="nav-btn" :class="{ on: view === 'activity' }" @click="view = 'activity'">{{ t("activity") }}</button>
          <button class="nav-btn" :class="{ on: view === 'settings' }" @click="view = 'settings'">{{ t("settings") }}</button>
        </nav>
        <div class="hdr-r">
          <button v-if="update" class="btn btn-sm btn-primary" :disabled="updating" @click="installUpdate" :title="updateError">
            {{ updating ? t("updating") : updateReady ? t("restartFor", { v: update.version }) : t("updateTo", { v: update.version }) }}
          </button>
          <span v-if="updateError" class="hdr-uperr" :title="updateError">{{ t("updateFailed") }} · <a href="#" @click.prevent="openUrl(`${siteBase}/desktop`)">{{ t("downloadManually") }}</a></span>
          <span v-if="usageText" class="hdr-usage mono">{{ usageText }}</span>
          <span class="pill" :class="auth.isPro ? 'pill-brand' : 'pill-muted'">{{ planLabel }}</span>
          <button class="btn btn-ghost btn-sm" :title="auth.user?.email" @click="openUrl(siteBase)">{{ t("openWebsite") }}</button>
        </div>
      </header>

      <div v-if="seat.state === 'elsewhere'" class="seat-bar">
        <span>{{ t("seatElsewhere", { name: seat.name || t("seatOtherComputer") }) }}<template v-if="seat.lastSeen"> · {{ t("seatLastSeen", { when: seatWhen(seat.lastSeen) }) }}</template></span>
        <span v-if="seat.message === 'move_limit'" class="seat-err">{{ seat.nextAllowed ? t("seatMoveLimitDate", { when: seatWhen(seat.nextAllowed) }) : t("seatMoveLimit") }}</span>
        <span v-else-if="seat.message" class="seat-err">{{ seat.message }}</span>
        <button class="btn btn-sm btn-primary" :disabled="moving || !!seat.nextAllowed" @click="moveHere">{{ moving ? t("seatMoving") : t("seatMoveHere") }}</button>
      </div>

      <main class="main">
        <ToolsView
          v-if="view === 'tools'"
          :tools="tools"
          :config="config"
          :loading="toolsLoading"
          :site-base="siteBase"
          @reload="loadTools(true)"
          @config-changed="loadConfig"
        />
        <ActivityView v-else-if="view === 'activity'" :active="view === 'activity'" />
        <SettingsView
          v-else
          :config="config"
          :auth="auth"
          :version="version"
          :site-base="siteBase"
          @save="saveConfig"
          @sign-out="signOut"
          @check-updates="checkForUpdates"
        />
      </main>

      <div v-if="dragOver" class="drop-veil"><div class="drop-msg">{{ t("dropHint") }}</div></div>

      <QuickAction
        v-if="droppedFiles.length"
        :files="droppedFiles"
        :tools="tools"
        :config="config"
        @close="droppedFiles = []"
        @config-changed="loadConfig"
      />
    </template>
  </div>
</template>

<style>
.hdr-uperr { font-size: 11.5px; color: var(--bad); max-width: 260px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.hdr-uperr a { color: var(--bad); text-decoration: underline; }
.gate { min-height: 100vh; display: grid; place-items: center; padding: 24px; }
.gate-card { width: 440px; max-width: 100%; padding: 32px 30px; display: flex; flex-direction: column; gap: 10px; align-items: flex-start; }
.gate-h { font-size: 22px; margin-top: 8px; }
.gate-p { margin: 0; color: var(--muted); }
.gate-who { font-size: 12px; color: var(--faint); }
.gate-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 10px; }
.app { height: 100vh; display: flex; flex-direction: column; position: relative; }
.boot { flex: 1; display: grid; place-items: center; }

.hdr {
  display: flex; align-items: center; gap: 18px; height: 56px; padding: 0 18px; flex: none;
  background: var(--card); border-bottom: 1px solid var(--cardline);
}
.hdr-l { display: flex; align-items: baseline; gap: 8px; min-width: 150px; }
.hdr-brand { font-size: 21px; }
.hdr-sub { font-size: 12px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; color: var(--muted); }
.hdr-nav { display: flex; gap: 2px; background: var(--inset); border: 1px solid var(--cardline); border-radius: 11px; padding: 3px; }
.nav-btn { height: 30px; padding: 0 14px; border: 0; border-radius: 8px; background: transparent; color: var(--muted); font-weight: 700; font-size: 12.5px; cursor: pointer; }
.nav-btn:hover { color: var(--text); }
.nav-btn.on { background: var(--card); color: var(--text); box-shadow: var(--shadow); }
.hdr-r { margin-left: auto; display: flex; align-items: center; gap: 10px; }
.hdr-usage { font-size: 12px; color: var(--muted); }

.main { flex: 1; min-height: 0; overflow: auto; }

.drop-veil { position: absolute; inset: 0; background: var(--brandSoft); border: 3px dashed var(--brand); display: grid; place-items: center; z-index: 40; pointer-events: none; }
.drop-msg { background: var(--card); border: 1px solid var(--brandLine); border-radius: 14px; padding: 14px 20px; font-weight: 800; box-shadow: var(--shadow); }
.seat-bar { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; padding: 10px 24px; background: color-mix(in srgb, var(--brand, #3b4bdb) 10%, transparent); border-bottom: 1px solid var(--line, #e5e7eb); font-size: 13px; }
.seat-bar .btn { margin-left: auto; }
.seat-err { color: var(--danger, #d92d20); }
</style>
