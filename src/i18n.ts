// In-app strings. Danish is the product's language and the default, as on
// the website; tool names and option labels come from the API in both.
import { computed, ref } from "vue";

export type Lang = "da" | "en";
export const lang = ref<Lang>("da");

const strings: Record<string, { da: string; en: string }> = {
  appName: { da: "PDF.dk Flow", en: "PDF.dk Flow" },
  loading: { da: "Indlæser…", en: "Loading…" },

  // login
  signInTitle: { da: "Log ind på din konto", en: "Sign in to your account" },
  signInSub: { da: "Samme konto som på pdf.dk", en: "The same account as on pdf.dk" },
  email: { da: "E-mail", en: "Email" },
  password: { da: "Adgangskode", en: "Password" },
  rememberMe: { da: "Husk min e-mail på denne computer", en: "Remember my e-mail on this computer" },
  signIn: { da: "Log ind", en: "Sign in" },
  browserLogin: { da: "Log ind via browser", en: "Sign in with your browser" },
  browserLoginSub: { da: "Google, Microsoft eller e-mail — som på pdf.dk", en: "Google, Microsoft or e-mail — same as on pdf.dk" },
  browserLoginWaiting: { da: "Browseren er åbnet. Log ind på pdf.dk, så kommer du automatisk tilbage hertil.", en: "Your browser is open. Sign in on pdf.dk and you will be sent back here automatically." },
  pasteCode: { da: "Kom du ikke tilbage? Indsæt koden fra siden:", en: "Not sent back? Paste the code from the page:" },
  useCode: { da: "Brug kode", en: "Use code" },
  orWithPassword: { da: "eller med adgangskode", en: "or with a password" },
  proRequiredTitle: { da: "PDF.dk Flow kræver Pro", en: "PDF.dk Flow needs Pro" },
  proRequiredBody: { da: "Overvågede mapper og træk-og-slip kører gennem din konto. Start en gratis prøveperiode på pdf.dk — appen låses op med det samme.", en: "Watched folders and drag-and-drop run through your account. Start a free trial on pdf.dk and the app unlocks right away." },
  seePrices: { da: "Se priser på pdf.dk", en: "See prices on pdf.dk" },
  checkAgain: { da: "Tjek igen", en: "Check again" },
  retry: { da: "Prøv igen", en: "Retry" },
  openInClient: { da: "Åbn i Desktop", en: "Open in Desktop" },
  openInClientTip: { da: "Åbn filen i PDF.dk Desktop og se, hvorfor den fejlede. Ret den, gem, og tryk Prøv igen.", en: "Open the file in PDF.dk Desktop and see why it failed. Fix it, save, then press Retry." },
  checkInClient: { da: "Tjek i Desktop", en: "Check in Desktop" },
  checkInClientTip: { da: "Åbn resultatet i PDF.dk Desktop og tjek det, før det sendes videre", en: "Open the result in PDF.dk Desktop and check it before it goes on" },
  clientMissing: { da: "PDF.dk Desktop er ikke installeret på denne computer.", en: "PDF.dk Desktop isn't installed on this computer." },
  getClient: { da: "Hent PDF.dk Desktop", en: "Get PDF.dk Desktop" },
  outputWhere: { da: "Resultater i", en: "Results in" },
  outputSubfolder: { da: "Processed/-mappen", en: "Processed/ subfolder" },
  outputSame: { da: "samme mappe", en: "the same folder" },
  outputCustom: { da: "anden mappe…", en: "another folder…" },
  signingIn: { da: "Logger ind…", en: "Signing in…" },
  createAccount: { da: "Opret gratis konto", en: "Create a free account" },
  forgotPassword: { da: "Glemt adgangskode?", en: "Forgot password?" },
  planNote: { da: "Gratis: 20 kørsler pr. måned · Pro: ubegrænset", en: "Free: 20 runs per month · Pro: unlimited" },
  loginFailed: { da: "Kunne ikke logge ind", en: "Could not sign in" },

  // header / nav
  tools: { da: "Værktøjer", en: "Tools" },
  activity: { da: "Aktivitet", en: "Activity" },
  settings: { da: "Indstillinger", en: "Settings" },
  openWebsite: { da: "Åbn pdf.dk", en: "Open pdf.dk" },
  signOut: { da: "Log ud", en: "Sign out" },
  unlimited: { da: "Ubegrænset", en: "Unlimited" },
  runsThisMonth: { da: "kørsler denne måned", en: "runs this month" },
  updateTo: { da: "Opdater til v{v}", en: "Update to v{v}" },
  updating: { da: "Opdaterer…", en: "Updating…" },
  restartFor: { da: "Genstart til v{v}", en: "Restart for v{v}" },
  autoUpdate: { da: "Automatiske opdateringer", en: "Automatic updates" },
  autoUpdateDesc: { da: "Nye versioner installeres af sig selv, når ingen filer er i gang. Genstart, når appen beder om det.", en: "New versions install by themselves when no files are running. Restart when the app asks." },

  // tools view
  watchedFolders: { da: "Overvågede mapper", en: "Watched folders" },
  watchedFoldersDesc: { da: "Læg filer i en mappe, så behandles de automatisk og lander i Processed/. Originalen flyttes til Originals/.", en: "Drop files into a folder and they are processed automatically into Processed/. The original moves to Originals/." },
  dropHint: { da: "Eller træk filer herind for at behandle dem med det samme", en: "Or drag files here to process them right away" },
  enable: { da: "Vælg mappe", en: "Choose folder" },
  changeFolder: { da: "Skift mappe", en: "Change folder" },
  addFolder: { da: "Mappe", en: "Folder" },
  removeFolder: { da: "Fjern", en: "Remove" },
  folderOptions: { da: "Indstillinger for denne mappe", en: "Options for this folder" },
  defaultOptions: { da: "Standardindstillinger (træk og slip, Åbn med)", en: "Default options (drag and drop, Open with)" },
  disable: { da: "Slå fra", en: "Disable" },
  options: { da: "Indstillinger", en: "Options" },
  accepts: { da: "Tager", en: "Takes" },
  enabledCount: { da: "{n} mapper overvåges", en: "{n} folders watched" },
  noTools: { da: "Ingen værktøjer kunne hentes. Tjek forbindelsen og prøv igen.", en: "No tools could be loaded. Check the connection and try again." },
  refreshTools: { da: "Hent værktøjer igen", en: "Reload tools" },
  filterTools: { da: "Søg værktøj…", en: "Search tools…" },
  showAll: { da: "Alle", en: "All" },
  onlyEnabled: { da: "Kun aktive", en: "Enabled only" },

  // options modal
  optionsFor: { da: "Indstillinger for {tool}", en: "Options for {tool}" },
  noOptions: { da: "Dette værktøj har ingen indstillinger.", en: "This tool has no options." },
  cancel: { da: "Annullér", en: "Cancel" },
  save: { da: "Gem", en: "Save" },
  required: { da: "påkrævet", en: "required" },

  // quick action (dropped / opened files)
  quickTitle: { da: "Hvad skal der ske med filerne?", en: "What should happen to the files?" },
  quickSub: { da: "{n} filer · resultatet gemmes ved siden af originalen", en: "{n} files · the result is saved next to the original" },
  run: { da: "Kør", en: "Run" },
  running: { da: "Kører…", en: "Running…" },
  combineIntoOne: { da: "Saml alle {n} filer i én PDF (kun værktøjer der tager flere filer)", en: "Combine all {n} files into one PDF (tools that take several files)" },
  noToolForFile: { da: "Intet værktøj tager denne filtype", en: "No tool takes this file type" },
  done: { da: "Færdig", en: "Done" },
  failed: { da: "Fejlede", en: "Failed" },
  showInFolder: { da: "Vis i mappe", en: "Show in folder" },
  close: { da: "Luk", en: "Close" },

  // activity
  recentJobs: { da: "Seneste kørsler", en: "Recent runs" },
  noJobs: { da: "Ingen kørsler endnu. Læg en fil i en overvåget mappe, eller træk en fil ind i appen.", en: "No runs yet. Drop a file into a watched folder, or drag a file into the app." },
  logs: { da: "Log", en: "Log" },
  clearLogs: { da: "Ryd", en: "Clear" },
  statusQueued: { da: "I kø", en: "Queued" },
  statusUploading: { da: "Uploader", en: "Uploading" },
  statusProcessing: { da: "Behandler", en: "Processing" },
  statusCompleted: { da: "Færdig", en: "Done" },
  statusFailed: { da: "Fejlede", en: "Failed" },

  // settings
  general: { da: "Generelt", en: "General" },
  language: { da: "Sprog", en: "Language" },
  languageDesc: { da: "Valgt efter computerens sprog ved første start. Værktøjsnavne og indstillinger følger med.", en: "Picked from the computer's language at first start. Tool names and options follow." },
  theme: { da: "Udseende", en: "Appearance" },
  themeSystem: { da: "Som systemet", en: "Follow system" },
  themeLight: { da: "Lyst", en: "Light" },
  themeDark: { da: "Mørkt", en: "Dark" },
  startOnLogin: { da: "Start ved login", en: "Start at login" },
  startOnLoginDesc: { da: "Appen starter i baggrunden, når du logger ind på computeren.", en: "The app starts in the background when you sign in to the computer." },
  notifications: { da: "Notifikationer", en: "Notifications" },
  notificationsDesc: { da: "Besked når en fil er færdig eller fejler.", en: "A notice when a file finishes or fails." },
  account: { da: "Konto", en: "Account" },
  plan: { da: "Abonnement", en: "Plan" },
  managePlan: { da: "Administrér på pdf.dk", en: "Manage on pdf.dk" },
  about: { da: "Om", en: "About" },
  version: { da: "Version", en: "Version" },
  processedVia: { da: "Filer behandles på pdf.dk's servere i Norden og slettes automatisk bagefter.", en: "Files are processed on pdf.dk's servers in the Nordics and deleted automatically afterwards." },
  checkUpdates: { da: "Søg efter opdatering", en: "Check for updates" },
  upToDate: { da: "Du har den nyeste version", en: "You have the latest version" },
  updateFailed: { da: "Opdatering fejlede", en: "Update failed" },

  // one Pro account, one computer
  seatElsewhere: { da: "Din Pro-licens bruges på »{name}« – mapperne kører ikke på denne computer", en: "Your Pro licence is in use on “{name}” – folders don't run on this computer" },
  seatOtherComputer: { da: "en anden computer", en: "another computer" },
  seatLastSeen: { da: "sidst set {when}", en: "last seen {when}" },
  seatMoveHere: { da: "Brug Pro her i stedet", en: "Use Pro here instead" },
  seatMoving: { da: "Flytter …", en: "Moving …" },
  seatMoveLimit: { da: "Pro er flyttet for mange gange på kort tid. Prøv igen senere.", en: "Pro has been moved too often recently. Try again later." },
  seatMoveLimitDate: { da: "Pro er flyttet for mange gange. Du kan flytte den igen fra {when}.", en: "Pro has been moved too often. You can move it again from {when}." },
  downloadManually: { da: "hent manuelt", en: "download manually" },

  // desktop widgets (0.4.0)
  widgets: { da: "Skrivebordswidgets", en: "Widgets" },
  widgetsDesc: { da: "Små felter på skrivebordet: slip filer på dem, og træk resultatet direkte videre til en mail eller en mappe.", en: "Small tiles on your screen: drop files on them and drag the result straight on into an e-mail or a folder." },
  widgetCreate: { da: "Sæt på skrivebordet", en: "Add widget" },
  widgetShort: { da: "Widget", en: "Widget" },
  widgetCreated: { da: "Widget lagt på skrivebordet", en: "Widget added" },
  widgetShow: { da: "Vis", en: "Show" },
  widgetOptions: { da: "Indstillinger for denne widget", en: "Options for this widget" },
  widgetRemove: { da: "Fjern fra skrivebordet", en: "Remove widget" },
  widgetCombine: { da: "Saml alle filer i én PDF", en: "Combine all files into one PDF" },
  widgetNone: { da: "Ingen widgets endnu. Tryk »Sæt på skrivebordet« på et værktøj.", en: "No widgets yet. Press \"Add widget\" on a tool." },
  widgetDrop: { da: "Slip filer her", en: "Drop files here" },
  widgetRunning: { da: "Behandler {n} fil(er)…", en: "Processing {n} file(s)…" },
  widgetDragOut: { da: "Træk filen videre", en: "Drag the file onwards" },
  widgetDragHint: { da: "Træk til en mail eller mappe · klik: vis i Finder", en: "Drag into an e-mail or folder · click: reveal" },
  widgetClear: { da: "Ryd", en: "Clear" },
  widgetFailed: { da: "Fejlede – se Aktivitet i appen", en: "Failed – see Activity in the app" },
  widgetOnly: { da: "Kun {ext}", en: "Only {ext}" },
  widgetSkipped: { da: "{n} fil(er) sprunget over", en: "{n} file(s) skipped" },
  widgetDragFailed: { da: "Kunne ikke starte træk – filen ligger ved originalen", en: "Could not start the drag – the file is next to the original" },
  widgetSignIn: { da: "Log ind i PDF.dk Flow for at bruge widgetten", en: "Sign in to PDF.dk Flow to use the widget" },
  widgetNeedsPro: { da: "Widgets kræver PDF.dk Pro", en: "Widgets need PDF.dk Pro" },
  widgetOpenApp: { da: "Åbn appen", en: "Open the app" },
};

export function t(key: string, vars?: Record<string, string | number>): string {
  const entry = strings[key];
  let s = entry ? entry[lang.value] : key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) s = s.replace(`{${k}}`, String(v));
  }
  return s;
}

/** Pick the current language from a {da,en} object the API sends. */
export function bi(obj: { da?: string; en?: string } | null | undefined): string {
  if (!obj) return "";
  return (lang.value === "en" ? obj.en || obj.da : obj.da || obj.en) || "";
}

export const isDa = computed(() => lang.value === "da");
