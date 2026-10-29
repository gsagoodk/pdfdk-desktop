<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { t } from "../i18n";
import { baseName, type Job } from "../types";

defineProps<{ active: boolean }>();
const jobs = ref<Job[]>([]);
const logs = ref<string[]>([]);
const showLogs = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;

async function refresh() {
  jobs.value = await invoke<Job[]>("get_jobs");
  if (showLogs.value) logs.value = await invoke<string[]>("get_logs");
}
const retrying = ref<Record<string, boolean>>({});
async function retry(j: Job) {
  retrying.value[j.id] = true;
  try { await invoke("retry_job", { jobId: j.id }); } catch { /* the job row shows the error */ }
  finally { retrying.value[j.id] = false; await refresh(); }
}
// PDF.dk Desktop (the PDF app, once PDF.dk Client): a failed file opens there with why it failed (it reads the job file next to
// it); a finished PDF can be checked there before it goes on.
const clientMissing = ref(false);
async function openInClient(path: string) {
  try {
    await invoke("open_in_client", { path });
  } catch (e) {
    if (String(e).includes("not_installed")) clientMissing.value = true;
  }
}
async function getClient() {
  openUrl(`${await invoke<string>("get_site_base")}/desktop`);
}
const isPdf = (p?: string | null) => !!p && /\.pdf$/i.test(p);
async function clearLogs() {
  await invoke("clear_logs");
  logs.value = [];
}
function when(ts: number) {
  return new Date(ts * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}
function statusPill(s: Job["status"]) {
  return { queued: "pill-muted", processing: "pill-brand", completed: "pill-ok", failed: "pill-bad" }[s];
}
function statusText(s: Job["status"]) {
  return { queued: t("statusQueued"), processing: t("statusProcessing"), completed: t("statusCompleted"), failed: t("statusFailed") }[s];
}

onMounted(() => { refresh(); timer = setInterval(refresh, 2000); });
onBeforeUnmount(() => { if (timer) clearInterval(timer); });
watch(showLogs, (v) => { if (v) refresh(); });
</script>

<template>
  <div class="av">
    <div class="av-top">
      <h2 class="av-h">{{ t("recentJobs") }}</h2>
      <button class="btn btn-sm btn-ghost" @click="showLogs = !showLogs">{{ t("logs") }}</button>
    </div>

    <div v-if="clientMissing" class="av-empty card">{{ t("clientMissing") }} <a href="#" @click.prevent="getClient">{{ t("getClient") }}</a></div>
    <div v-if="!jobs.length" class="av-empty card">{{ t("noJobs") }}</div>

    <div v-else class="av-list card">
      <div v-for="j in jobs" :key="j.id" class="av-row">
        <span class="pill" :class="statusPill(j.status)">{{ statusText(j.status) }}</span>
        <div class="av-txt">
          <div class="av-name mono" :title="j.outputFile || j.inputFile">{{ baseName(j.outputFile || j.inputFile) }}</div>
          <div class="av-sub">{{ j.toolName }} · {{ when(j.createdAt) }}<span v-if="j.error"> · <span class="av-err">{{ j.error }}</span></span></div>
        </div>
        <span v-if="j.status === 'processing'" class="spin" style="width:14px;height:14px"></span>
        <button v-if="j.status === 'failed' && isPdf(j.inputFile)" class="btn btn-sm btn-ghost" :title="t('openInClientTip')" @click="openInClient(j.inputFile)">{{ t("openInClient") }}</button>
        <button v-else-if="j.status === 'completed' && isPdf(j.outputFile)" class="btn btn-sm btn-ghost" :title="t('checkInClientTip')" @click="openInClient(j.outputFile!)">{{ t("checkInClient") }}</button>
        <button v-if="j.status === 'failed'" class="btn btn-sm" :disabled="retrying[j.id]" @click="retry(j)">{{ t("retry") }}</button>
        <button v-if="j.outputFile" class="btn btn-sm btn-ghost" @click="revealItemInDir(j.outputFile!)">{{ t("showInFolder") }}</button>
      </div>
    </div>

    <div v-if="showLogs" class="av-logs card">
      <div class="av-logs-h">
        <span>{{ t("logs") }}</span>
        <button class="btn btn-sm btn-ghost" @click="clearLogs">{{ t("clearLogs") }}</button>
      </div>
      <pre class="av-pre mono">{{ logs.join("\n") }}</pre>
    </div>
  </div>
</template>

<style scoped>
.av { padding: 22px 24px 40px; max-width: 980px; margin: 0 auto; display: flex; flex-direction: column; gap: 12px; }
.av-top { display: flex; align-items: center; justify-content: space-between; }
.av-h { font-size: 20px; }
.av-empty { padding: 26px; color: var(--muted); }
.av-list { display: flex; flex-direction: column; }
.av-row { display: flex; align-items: center; gap: 12px; padding: 11px 14px; border-bottom: 1px solid var(--cardline); }
.av-row:last-child { border-bottom: 0; }
.av-txt { flex: 1; min-width: 0; }
.av-name { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.av-sub { font-size: 12px; color: var(--muted); }
.av-err { color: var(--bad); }
.av-logs { display: flex; flex-direction: column; }
.av-logs-h { display: flex; align-items: center; justify-content: space-between; padding: 8px 8px 8px 14px; border-bottom: 1px solid var(--cardline); font-weight: 700; font-size: 12.5px; }
.av-pre { margin: 0; padding: 12px 14px; font-size: 11.5px; color: var(--text2); max-height: 280px; overflow: auto; white-space: pre-wrap; user-select: text; }
</style>
