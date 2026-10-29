<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../i18n";
import type { AuthState } from "../types";

const props = defineProps<{ siteBase: string }>();
const emit = defineEmits<{ (e: "signed-in", auth: AuthState): void }>();

const email = ref("");
const password = ref("");
const remember = ref(false);
const error = ref("");
const busy = ref(false);
const waiting = ref(false);       // browser sign-in in progress
const code = ref("");
const showPassword = ref(false);
let unAuth: UnlistenFn | null = null;
let unErr: UnlistenFn | null = null;

/** The website signs the user in (any method) and hands a token back through pdfdk://auth. */
async function browserLogin() {
  error.value = "";
  try {
    await invoke("start_browser_login");
    waiting.value = true;
  } catch (e: any) {
    error.value = String(e);
  }
}
async function useCode() {
  if (!code.value.trim()) return;
  busy.value = true; error.value = "";
  try {
    const auth = await invoke<AuthState>("login_with_token", { token: code.value.trim() });
    emit("signed-in", auth);
  } catch (e: any) {
    error.value = String(e).replace(/^Error: /, "");
  } finally {
    busy.value = false;
  }
}
onBeforeUnmount(() => { unAuth?.(); unErr?.(); });

onMounted(async () => {
  unAuth = await listen("auth-changed", async () => {
    const auth = await invoke<AuthState>("check_auth");
    if (auth.isAuthenticated) emit("signed-in", auth);
  });
  unErr = await listen<string>("auth-error", (e) => { waiting.value = false; error.value = e.payload; });
  try {
    const saved = await invoke<{ email: string; password: string } | null>("get_saved_credentials");
    if (saved) { email.value = saved.email; remember.value = true; }   // e-mail only — the password is never stored (0.3.4)
  } catch { /* none saved */ }
});

async function submit() {
  error.value = "";
  busy.value = true;
  try {
    const auth = await invoke<AuthState>("login", { email: email.value, password: password.value, remember: remember.value });
    emit("signed-in", auth);
  } catch (e: any) {
    error.value = String(e).replace(/^Error: /, "");
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="login">
    <div class="login-card card">
      <div class="login-brand">
        <span class="pdk-brand" style="font-size:34px">pdf<i class="pdk-dot"></i>dk</span>
        <span class="login-sub">Flow</span>
      </div>
      <h1 class="login-h">{{ t("signInTitle") }}</h1>
      <p class="login-p">{{ t("signInSub") }}</p>

      <div class="login-browser">
        <button type="button" class="btn btn-primary btn-lg" style="width:100%" :disabled="busy" @click="browserLogin">{{ t("browserLogin") }}</button>
        <p class="login-note" style="margin:6px 0 0">{{ t("browserLoginSub") }}</p>
        <div v-if="waiting" class="login-wait">
          <div class="login-wait-row"><span class="spin" style="width:14px;height:14px"></span><span>{{ t("browserLoginWaiting") }}</span></div>
          <label class="login-code">
            <span>{{ t("pasteCode") }}</span>
            <span class="login-code-row">
              <input v-model="code" class="input mono" type="text" autocomplete="off" spellcheck="false" @keydown.enter.prevent="useCode" />
              <button type="button" class="btn" :disabled="busy || !code.trim()" @click="useCode">{{ t("useCode") }}</button>
            </span>
          </label>
        </div>
        <div v-if="error && !showPassword" class="login-err">{{ error }}</div>
      </div>

      <button type="button" class="login-toggle" @click="showPassword = !showPassword">{{ t("orWithPassword") }}</button>

      <form v-if="showPassword" class="login-form" @submit.prevent="submit">
        <div class="field">
          <label for="email">{{ t("email") }}</label>
          <input id="email" v-model="email" class="input" type="email" autocomplete="username" required :disabled="busy" />
        </div>
        <div class="field">
          <label for="password">{{ t("password") }}</label>
          <input id="password" v-model="password" class="input" type="password" autocomplete="current-password" required :disabled="busy" />
        </div>
        <label class="remember">
          <button type="button" class="switch" :class="{ on: remember }" :aria-pressed="remember" @click="remember = !remember"></button>
          <span>{{ t("rememberMe") }}</span>
        </label>
        <div v-if="error" class="login-err">{{ error }}</div>
        <button type="submit" class="btn btn-lg" :disabled="busy">
          <span v-if="busy" class="spin" style="width:14px;height:14px;border-width:2px;border-top-color:#fff"></span>
          {{ busy ? t("signingIn") : t("signIn") }}
        </button>
      </form>

      <p class="login-note">{{ t("planNote") }}</p>
      <p class="login-links">
        <a href="#" @click.prevent="openUrl(`${props.siteBase}/register`)">{{ t("createAccount") }}</a>
        <span>·</span>
        <a href="#" @click.prevent="openUrl(`${props.siteBase}/forgot-password`)">{{ t("forgotPassword") }}</a>
      </p>
    </div>
  </div>
</template>

<style scoped>
.login-browser { margin: 4px 0 10px; }
.login-wait { margin-top: 12px; padding: 12px; border-radius: 10px; background: var(--inset); border: 1px solid var(--cardline); font-size: 12.5px; color: var(--muted); }
.login-wait-row { display: flex; gap: 10px; align-items: center; }
.login-code { display: flex; flex-direction: column; gap: 6px; margin-top: 10px; }
.login-code-row { display: flex; gap: 6px; }
.login-code-row .input { flex: 1; height: 32px; font-size: 12px; }
.login-toggle { border: 0; background: transparent; color: var(--muted); font-size: 12.5px; cursor: pointer; padding: 6px 0; text-decoration: underline; text-underline-offset: 3px; }
.login-toggle:hover { color: var(--text); }
.login { height: 100vh; display: grid; place-items: center; padding: 24px; background: radial-gradient(900px 500px at 50% -10%, var(--brandSoft), transparent 70%), var(--bg); }
.login-card { width: 380px; padding: 30px 30px 24px; }
.login-brand { display: flex; align-items: baseline; gap: 8px; margin-bottom: 18px; }
.login-sub { font-size: 12px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; color: var(--muted); }
.login-h { font-size: 19px; }
.login-p { margin: 4px 0 18px; color: var(--muted); }
.login-form { display: flex; flex-direction: column; gap: 14px; }
.remember { display: flex; align-items: center; gap: 10px; font-size: 12.5px; color: var(--text2); cursor: pointer; }
.login-err { background: var(--badSoft); color: var(--bad); border-radius: 9px; padding: 9px 11px; font-size: 12.5px; }
.login-note { margin: 18px 0 6px; font-size: 12px; color: var(--muted); text-align: center; }
.login-links { margin: 0; font-size: 12.5px; text-align: center; display: flex; justify-content: center; gap: 8px; color: var(--faint); }
</style>
