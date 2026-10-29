// Browser preview of the UI without Tauri: a fake `window.__TAURI_INTERNALS__`
// that answers the commands App.vue and the plugins call. Only loaded by
// main.ts in `vite dev` when the real bridge is absent. Nothing here ships.
import fixture from "./tools.fixture.json";

type Handler = (args: any) => any;

const config = {
  widgets: [{ id: "w1", toolId: "compress", options: { quality: "high" }, combine: false, x: 40, y: 80 }] as any[],
  version: 2,
  general: { startOnLogin: true, startMinimized: true, showNotifications: true, language: "da", theme: "system" },
  tools: [
    { id: "compress", key: "compress", enabled: true, folderPath: "/Users/demo/PDF.dk/Komprimer", outputMode: "subfolder", options: { quality: "high" }, accepts: ["pdf"], output: "pdf", name: { da: "Komprimer PDF", en: "Compress PDF" } },
    { id: "outline", key: "outline", enabled: true, folderPath: "/Users/demo/PDF.dk/Kurver", outputMode: "subfolder", options: {}, accepts: ["pdf"], output: "pdf", name: { da: "Tekst til kurver", en: "Fonts to outlines" } },
    { id: "color-profile", key: "color-profile", enabled: true, folderPath: "/Users/demo/PDF.dk/Farveprofil/Konverter", outputMode: "subfolder", options: { mode: "convert", profile: "fogra39", intent: "RelativeColorimetric" }, accepts: ["pdf"], output: "pdf", name: { da: "Farveprofil", en: "Colour profile" } },
    { id: "color-profile", key: "color-profile-2", enabled: true, folderPath: "/Users/demo/PDF.dk/Farveprofil/Tjek", outputMode: "subfolder", options: { mode: "check", profile: "fogra39", intent: "RelativeColorimetric" }, accepts: ["pdf"], output: "pdf", name: { da: "Farveprofil", en: "Colour profile" } },
  ] as any[],
};

const signedIn = new URLSearchParams(location.search).get("login") !== "0";
const auth = () => ({
  isAuthenticated: signedIn, isPro: true, token: signedIn ? "mock" : null, plan: signedIn ? "pro" : null,
  user: signedIn ? { id: 1, email: "demo@pdf.dk", name: "Demo Bruger", isSuperadmin: false, role: "user" } : null,
  jobsLimit: 20, jobsUsed: 7, jobsRemaining: 13, maxFileSizeMb: 500, isUnlimited: signedIn,
});

const jobs = [
  { id: "1", toolId: "compress", toolName: "Komprimer PDF", inputFile: "/Users/demo/PDF.dk/Komprimer/katalog-2026.pdf", outputFile: "/Users/demo/PDF.dk/Komprimer/Processed/katalog-2026_compress.pdf", status: "completed", error: null, createdAt: Date.now() / 1000 - 120, completedAt: Date.now() / 1000 - 100 },
  { id: "2", toolId: "outline", toolName: "Tekst til kurver", inputFile: "/Users/demo/PDF.dk/Kurver/flyer.pdf", outputFile: null, status: "processing", error: null, createdAt: Date.now() / 1000 - 20, completedAt: null },
  { id: "3", toolId: "unlock", toolName: "Lås PDF op", inputFile: "/Users/demo/Downloads/04-05-2026.pdf", outputFile: null, status: "failed", error: "PDF kræver adgangskode. Prøv igen med den korrekte adgangskode.", createdAt: Date.now() / 1000 - 600, completedAt: Date.now() / 1000 - 590 },
];

const handlers: Record<string, Handler> = {
  "plugin:app|version": () => "0.3.0-preview",
  "plugin:event|listen": () => 1,
  "plugin:event|unlisten": () => null,
  "plugin:updater|check": () => null,
  "plugin:opener|open_url": ({ url }) => { window.open(url, "_blank"); return null; },
  "plugin:opener|reveal_item_in_dir": () => null,
  "plugin:dialog|open": () => "/Users/demo/PDF.dk/Ny mappe",
  get_config: () => config,
  widget_list: () => config.widgets,
  widget_get: ({ id }) => { const w = config.widgets.find((x: any) => x.id === id) || config.widgets[0]; return { widget: w, tool: fixture.data.tools.find((t: any) => t.id === w.toolId) || fixture.data.tools[0], language: "da", theme: "system" }; },
  widget_create: ({ toolId }) => { const w = { id: Math.random().toString(36).slice(2, 10), toolId, options: {}, combine: true, x: null, y: null }; config.widgets.push(w); return w; },
  widget_remove: ({ id }) => { config.widgets = config.widgets.filter((x: any) => x.id !== id); return null; },
  widget_show: () => null,
  widget_update: ({ id, options, combine }) => { const w = config.widgets.find((x: any) => x.id === id); if (w) { w.options = options; if (combine != null) w.combine = combine; } return null; },
  widget_process: async ({ paths }) => { await new Promise((r) => setTimeout(r, 1500)); return paths.map((p: string) => ({ input: p, output: p.replace(/(\.[^.]+)$/, "_compressed$1"), error: null })); },
  widget_show_main: () => null,
  "plugin:drag|start_drag": () => null,
  save_config: ({ newConfig }) => { Object.assign(config, newConfig); return null; },
  get_site_base: () => "https://dev.pdf.dk",
  check_auth: auth,
  get_auth_state: auth,
  login: ({ email }) => { if (!email.includes("@")) throw "Invalid credentials"; return { ...auth(), isAuthenticated: true }; },
  logout: () => null,
  get_saved_credentials: () => null,
  get_available_tools: () => fixture.data.tools,
  start_watchers: () => null,
  // ?drop=1 simulates files handed over by the OS (dock drop / open-with) on start
  take_pending_files: () => { const d = new URLSearchParams(location.search).get("drop"); return d === "img" ? ["/Users/demo/Downloads/IMG_7731.jpeg", "/Users/demo/Downloads/IMG_7732.jpeg", "/Users/demo/Downloads/IMG_7733.heic"] : d ? ["/Users/demo/Downloads/Årsrapport 2025.pdf", "/Users/demo/Downloads/bilag-7.pdf"] : []; },
  enable_tool: ({ toolId, folderPath, key }) => {
    const def = fixture.data.tools.find((t: any) => t.id === toolId);
    if (config.tools.some((t) => t.enabled && t.folderPath === folderPath && t.key !== key)) throw `${folderPath} is already watched`;
    const existing = key ? config.tools.find((t) => t.key === key) : config.tools.find((t) => t.id === toolId && !t.enabled);
    if (existing) Object.assign(existing, { enabled: true, folderPath });
    else {
      const n = config.tools.filter((t) => t.id === toolId).length;
      config.tools.push({ id: toolId, key: n ? `${toolId}-${n + 1}` : toolId, enabled: true, folderPath, outputMode: "subfolder", options: {}, accepts: def?.accepts, output: def?.output, name: def?.name });
    }
    return null;
  },
  disable_tool: ({ key }) => {
    const i = config.tools.findIndex((t) => t.key === key);
    if (i < 0) return null;
    const others = config.tools.some((t) => t.id === config.tools[i].id && t.key !== key);
    if (others) config.tools.splice(i, 1); else config.tools[i].enabled = false;
    return null;
  },
  update_tool_options: ({ toolId, options, key }) => { const t = key ? config.tools.find((x) => x.key === key) : config.tools.find((x) => x.id === toolId); if (t) t.options = options; return null; },
  process_files: async ({ paths, combine }) => { await new Promise((r) => setTimeout(r, 1200)); if (combine && paths.length > 1) return [{ input: paths.join(", "), output: paths[0].replace(/(\.[^.]+)$/, "_samlet.pdf"), error: null }]; return paths.map((p: string) => ({ input: p, output: p.replace(/(\.[^.]+)$/, "_tool$1"), error: null })); },
  get_jobs: () => jobs,
  start_browser_login: () => "https://dev.pdf.dk/desktop-login?state=mock",
  login_with_token: ({ token }) => { if (token !== "mock") throw "Unauthorized - please login again"; return { ...auth(), isAuthenticated: true }; },
  update_tool_output: ({ key, mode, path }) => { const t = config.tools.find((x) => x.key === key); if (t) t.outputMode = mode === "custom" ? { custom: path } : mode === "same" ? "same-folder" : "subfolder"; return null; },
  retry_job: async ({ jobId }) => { const j = jobs.find((x) => x.id === jobId); await new Promise((r) => setTimeout(r, 800)); if (j) { j.status = "completed"; j.error = null; j.outputFile = j.inputFile.replace(/(\.[^.]+)$/, "_retry$1"); } return null; },
  log_client: () => null,
  auto_update_test: () => false,
  get_logs: () => ["[12:00:01] Tools catalogue 2026-09-20 loaded: 38 tools", "[12:00:02] Watching /Users/demo/PDF.dk/Komprimer for compress"],
  clear_logs: () => null,
};

(window as any).__TAURI_INTERNALS__ = {
  invoke: async (cmd: string, args: any = {}) => {
    const h = handlers[cmd];
    if (!h) { console.warn("mock: unhandled command", cmd, args); return null; }
    return h(args);
  },
  transformCallback: (cb: any) => { const id = Math.floor(Math.random() * 1e9); (window as any)[`_${id}`] = cb; return id; },
  unregisterCallback: () => {},
  metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
  plugins: { path: { sep: "/", delimiter: ":" } },
};
console.info("PDF.dk Flow: browser preview mode (Tauri mocked)");
