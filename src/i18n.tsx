// ============================================================
//  جسر — نظام اللغات (عربي / إنجليزي) + الاتجاه (RTL / LTR)
//  اللغة الافتراضية = لغة النظام تلقائياً، والمستخدم يقدر يغيّرها.
// ============================================================
import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

// ---------- القاموس العربي ----------
const ar = {
  appTitle: "جسر — ألعاب ويندوز على الماك",
  appName: "جسر",
  tagline: "ألعاب ويندوز على الماك",
  preview: "نسخة تجريبية 0.1.0",
  openSource: "مشروع مفتوح المصدر",

  // التنقّل
  navHome: "الرئيسية",
  navLibrary: "المكتبة",
  navBottles: "القوارير",
  navSettings: "الإعدادات",
  navLogs: "السجل",

  // الرئيسية
  greeting: "أهلاً بك في جسر",
  greetingSub:
    "شغّل ألعاب ويندوز على ماك بضغطة وحدة — بدون تيرمنال وبدون إعدادات معقّدة.",
  installEngine: "ثبّت المحرك",
  addGame: "أضف لعبة",
  readiness: "جاهزية النظام",
  rosetta: "Rosetta 2",
  wineEngine: "محرك Wine",
  graphicsDxmt: "طبقة الرسومات (DXMT)",
  ready: "جاهز",
  installing: "قيد التثبيت…",
  notInstalled: "غير مثبت",
  yourLibrary: "مكتبتك",
  noGames: "فارغة — 0 لعبة",
  openLibrary: "افتح المكتبة",
  yourDevice: "جهازك",
  detectedAutomatically: "تُكتشف تلقائياً",
  checkDevice: "فحص الجهاز",
  themeAuto: "تلقائي",
  systemLabel: "النظام",
  chipLabel: "المعالج",
  memoryLabel: "الذاكرة",
  graphicsLabel: "الرسومات",
  coresSuffix: "أنوية",
  unavailable: "غير متوفر",
  scanningLabel: "جاري الفحص…",
  refresh: "تحديث",
  liveLabel: "مباشر",
  stepsTitle: "ابدأ في 3 خطوات",
  step1Title: "ثبّت المحرك",
  step1Desc: "ننزّل Wine وطبقات الرسومات تلقائياً",
  step2Title: "أضف لعبة",
  step2Desc: "من مكتبة ستيم أو ملف تثبيت .exe",
  step3Title: "اضغط العب",
  step3Desc: "ضبط تلقائي لأفضل أداء على جهازك",

  // المكتبة
  filterAll: "الكل",
  filterSteam: "ستيم",
  filterFiles: "ملفات",
  libraryEmptyTitle: "مكتبتك فاضية",
  libraryEmptyDesc: "أضف أول لعبة، وبنضبط كل شي تلقائياً — من ستيم أو ملف .exe.",
  exeFiles: "برامج ويندوز",
  allFiles: "كل الملفات",
  gamePlay: "شغّل",
  gameStop: "أوقف",
  gameRunningNow: "شغالة الآن",
  gamePlayed: "لعبتها {dur}",
  gameReveal: "إظهار في المجلد",
  gameRemove: "حذف",
  gameRemoveWarn: "متأكد؟ تنمسح من المكتبة فقط — ملفات اللعبة ما تنمسح.",
  gameRemoveYes: "امسحها من المكتبة",
  gameRemoveNo: "تراجع",
  gameAddedToast: "أُضيفت «{name}» للمكتبة ✓",
  gameExitToast: "انتهت «{name}» · مدة اللعب {dur}",
  gameStopToast: "أوقفت «{name}»",
  gameFailedToast: "«{name}» ما اشتغلت — السجل فيه التفاصيل",
  gameNeedBottle:
    "جهّز القارورة الرئيسية أول (صفحة القوارير) — بعدها زر التشغيل يصير جاهز.",
  libraryNoMatch: "ما فيه ألعاب بهذا التصنيف حالياً.",
  libraryNoteTitle: "وين ملفات ألعابي؟",
  libraryNoteDesc:
    "ملفات اللعبة تبقى بمكانها على الماك — جسر يشغلها من مكانها عبر قرص خاص لها فقط، وباقي ملفات جهازك تبقى بعيدة عن اللعبة (بديل Z: الآمن).",

  // القوارير
  bottleWhatTitle: "شنو القارورة؟",
  bottleWhatDesc:
    "القارورة = بيئة ويندوز مصغّرة داخل الماك، وكل ألعابك تشتغل فيها افتراضياً: الحفظ والإعدادات بمكان واحد آمن. وإذا احتاجت لعبة معينة عزل كامل، تقدر تعطيها قارورة خاصة فيها لحالها.",
  newBottle: "قارورة خاصة",
  bottleMainName: "القارورة الرئيسية",
  bottlePrivateName: "قارورة خاصة",
  bottleSetupTitle: "جهّز القارورة الرئيسية",
  bottleSetup: "جهّزها الآن",
  bottleSetupDesc:
    "بيئة ويندوز مصغّرة تُجهّز مرة وحدة (~دقيقة) — كل ألعابك تشتغل فيها.",
  bottleNeedEngine: "ثبّت محرك Wine أول — من الرئيسية أو الإعدادات.",
  bottleSettingUp: "جاري التجهيز…",
  bottleSettingUpHint: "أول تجهيز ياخذ شوي — لا تسكّر التطبيق.",
  bottleReadyNow: "جاهزة للاستخدام",
  bottleFailedTitle: "التجهيز ما كمّل",
  bottleRetry: "أعد المحاولة",
  bottleDevSim: "تخطَّ الفحص (وضع التطوير)",
  bottleDelete: "حذف",
  bottleDeleteWarn:
    "متأكد؟ راح ينمسح كل شي داخل هذي القارورة — بما فيه الحفظ.",
  bottleDeleteYes: "احذف نهائياً",
  bottleDeleteNo: "تراجع",

  // الإعدادات
  appearance: "المظهر",
  themeLight: "فاتح",
  themeDark: "داكن",
  themeSystem: "النظام",
  language: "اللغة",
  languageSystem: "لغة النظام",
  graphicsLayer: "طبقة الرسومات",
  font: "الخط",
  fontSystem: "خط النظام",
  fontDesc: "التطبيق يستخدم خط النظام (SF على الماك) عشان يطلع طبيعي",
  autoSaveNote: "الإعدادات تُحفظ تلقائياً بمجرد ما تغيّرها — بدون زر حفظ.",
  engineSection: "المحرك",
  install: "تثبيت",
  saved: "تم الحفظ",
  comingSoon: "قريباً",
  showInDock: "إظهار في الإعدادات المتقدمة",
  advanced: "متقدّم",

  // السجل
  logsTitle: "السجل والتشخيص",
  copyReport: "انسخ التقرير",
  copied: "تم النسخ ✓",
  clearLogs: "تفريغ",
  logsEmpty: "ما فيه أحداث بعد — أي شي يصير بينكتب هنا.",

  // التنزيل
  dlPreparing: "جاري التحضير…",
  dlVerifying: "نتحقق من سلامة الملف (بصمة SHA-256)…",
  dlExtracting: "جاري فك الضغط وتثبيت المحرك…",
  filesSuffix: "ملف",
  dlDone: "تم التنزيل والتحقق ✓",
  dlReady: "محرك Wine جاهز ✓",
  dlFailed: "فشل التنزيل",
  dlInstallFailed: "فشل تثبيت المحرك",
  dlCancelled: "أُلغي التنزيل — تقدر تكمّل من وين وقف",
  dlCancel: "إلغاء",

  // رسائل
  soonEngine: "تثبيت المحرك بيجي في الخطوة الجاية 🚧",
  soonGames: "إضافة الألعاب بيجي في الخطوة الجاية 🚧",
  soonDevice: "فحص الجهاز يحتاج نشتغل على ماك 🍎",
  soonBottles: "إدارة القوارير بيجي في الخطوة الجاية 🚧",
  soonLogs: "التشخيص الكامل بيجي في الخطوة الجاية 🚧",

  // سجل الحدث (نشأ في Rust بمفاتيح — نترجمها هنا)
  "log.startup": "بدأ جسر — {os}/{arch}",
  "log.scanDone": "اكتمل فحص الجهاز",
  "log.engineFound": "محرك Wine: {path}",
  "log.engineNotFound": "محرك Wine غير مثبت",
  "log.rosettaReady": "Rosetta 2 جاهزة ✓",
  "log.rosettaMissing": "Rosetta 2 غير مثبتة — نحتاجها لتشغيل ألعاب ويندوز",
  "log.cleared": "تم تفريغ السجل",
  "log.downloadStarted": "بدأ تنزيل {name} {version} — {size}",
  "log.downloadDone": "اكتمل تنزيل {name} {version} — البصمة مطابقة ✓",
  "log.downloadFailed": "فشل تنزيل {name}: {error}",
  "log.downloadCancelled": "أُلغي تنزيل {name} — المنزّل محفوظ للاستكمال",
  "log.extractStarted": "بدأ فك ضغط {name} {version}…",
  "log.extractDone": "تم فك الضغط: {files} ملف ({size})",
  "log.extractFailed": "فشل فك ضغط {name}: {error}",
  "log.extractSkipped": "تخطّينا {count} رابطاً رمزياً أثناء الفك (خارج مجلد التثبيت أو غير مدعوم على ويندوز)",
  "log.engineReady": "المحرك جاهز ✓ — {path}",
  "log.bottleStarted": "بدأ تجهيز {name}…",
  "log.bottleReady": "{name} جاهزة ✓",
  "log.bottleFailed": "فشل تجهيز {name}: {error}",
  "log.bottleDeleted": "حُذفت {name}",
  "log.gameAdded": "أُضيفت {name} للمكتبة",
  "log.gameRemoved": "حُذفت {name} من المكتبة",
  "log.gameLaunching": "جاري تشغيل {name}…",
  "log.gameExited": "انتهت {name} — مدة اللعب {dur}",
  "log.gameStopped": "أوقفت {name} — مدة اللعب {dur}",
  "log.gameLaunchFailed": "فشل تشغيل {name}: {error}",

  // ستيم
  steamCardTitle: "ستيم — نسخة ويندوز",
  steamDesc:
    "ننزّل مثبّت Valve الرسمي ونجهّزه داخل القارورة الرئيسية. ملاحظة: هذي نسخة الويندوز من ستيم — مستقلة تماماً عن نسخة الماك اللي عندك.",
  steamNeedBottle:
    "جهّز القارورة الرئيسية أول (صفحة القوارير) — بعدها نكمّل تثبيت ستيم.",
  steamStepDownload: "الخطوة ١ — نزّل مثبّت ستيم (نسخة الويندوز) الرسمي",
  steamStepDownloadHint: "2.3 MB من سيرفرات Valve — نتحقق من البصمة تلقائياً.",
  steamDownloadBtn: "نزّل المثبّت",
  steamInstallerReady: "المثبّت جاهز ✓ — ثبّت نسخة الويندوز داخل القارورة.",
  steamStepInstall: "الخطوة ٢ — ثبّت ستيم (نسخة الويندوز) داخل القارورة",
  steamStepInstallHint:
    "هذي نسخة الويندوز من ستيم — مستقلّة عن ستيم الماك. بتنفتح نافذة المثبّت — كمّل الخطوات العادية (Next ثم Install). أول تشغيل ياخذ شوي عشان التحديثات.",
  steamInstallBtn: "ثبّت ستيم للويندوز",
  steamInstalling: "التثبيت شغّال… كمّل النافذة",
  steamStepRun: "ستيم (نسخة ويندوز) جاهز للتشغيل",
  steamRunBtn: "شغّل ستيم",
  steamRunningNow: "ستيم شغّال الآن",
  steamStop: "أوقف",
  steamNote:
    "من داخل ستيم: سجّل دخولك ونزّل لعبتك (جرّب LIMBO Demo المجانية) — بعدها تقدر تلعب مباشرة من ستيم، أو تضيف ملف اللعبة لجسر عشان تشغّلها بضغطة من هنا.",
  steamAutoDl: "ننزّل مثبّت ستيم الرسمي تلقائياً — ياخذ ثواني…",
  "log.steamInstalling": "بدأنا تثبيت ستيم داخل القارورة الرئيسية…",
  "log.steamInstalled": "تم تثبيت ستيم ✓ — جاهز للتشغيل",
  "log.steamInstallExited": "المثبّت انتهى بدون ما يظهر ستيم — جرّب مرة ثانية",
  "log.steamInstallFailed": "فشل تثبيت ستيم: {error}",
  "log.steamInstallStopped": "أوقفت تثبيت ستيم",
  "log.steamLaunching": "جاري تشغيل ستيم…",
  "log.steamExited": "انسكّر ستيم",
  "log.steamStopped": "أوقفت ستيم",
  "log.steamLaunchFailed": "فشل تشغيل ستيم: {error}",
};

export type TKey = keyof typeof ar;

// ---------- القاموس الإنجليزي ----------
const en: Record<TKey, string> = {
  appTitle: "Jisr — Windows games on Mac",
  appName: "Jisr",
  tagline: "Windows games on Mac",
  preview: "Preview 0.1.0",
  openSource: "Open source project",

  navHome: "Home",
  navLibrary: "Library",
  navBottles: "Bottles",
  navSettings: "Settings",
  navLogs: "Logs",

  greeting: "Welcome to Jisr",
  greetingSub:
    "Play Windows games on your Mac in one click — no terminal, no fiddly setup.",
  installEngine: "Install engine",
  addGame: "Add game",
  readiness: "System readiness",
  rosetta: "Rosetta 2",
  wineEngine: "Wine engine",
  graphicsDxmt: "Graphics layer (DXMT)",
  ready: "Ready",
  installing: "Installing…",
  notInstalled: "Not installed",
  yourLibrary: "Your library",
  noGames: "Empty — 0 games",
  openLibrary: "Open library",
  yourDevice: "Your device",
  detectedAutomatically: "Detected automatically",
  checkDevice: "Check device",
  themeAuto: "Auto",
  systemLabel: "System",
  chipLabel: "Chip",
  memoryLabel: "Memory",
  graphicsLabel: "Graphics",
  coresSuffix: "cores",
  unavailable: "Not available",
  scanningLabel: "Scanning…",
  refresh: "Refresh",
  liveLabel: "Live",
  stepsTitle: "Get started in 3 steps",
  step1Title: "Install the engine",
  step1Desc: "We download Wine and the graphics layers for you",
  step2Title: "Add a game",
  step2Desc: "From Steam or an .exe installer",
  step3Title: "Press play",
  step3Desc: "Auto-tuned for your machine",

  filterAll: "All",
  filterSteam: "Steam",
  filterFiles: "Files",
  libraryEmptyTitle: "Your library is empty",
  libraryEmptyDesc:
    "Add your first game and we'll handle the setup — from Steam or an .exe file.",
  exeFiles: "Windows apps",
  allFiles: "All files",
  gamePlay: "Play",
  gameStop: "Stop",
  gameRunningNow: "Running now",
  gamePlayed: "Played {dur}",
  gameReveal: "Show in folder",
  gameRemove: "Remove",
  gameRemoveWarn:
    "Sure? Removes it from the library only — game files are not deleted.",
  gameRemoveYes: "Remove from library",
  gameRemoveNo: "Cancel",
  gameAddedToast: "Added “{name}” to your library ✓",
  gameExitToast: "“{name}” ended · played {dur}",
  gameStopToast: "Stopped “{name}”",
  gameFailedToast: "“{name}” didn't start — check the log",
  gameNeedBottle:
    "Set up the Main Bottle first (Bottles page) — then Play becomes ready.",
  libraryNoMatch: "No games in this category yet.",
  libraryNoteTitle: "Where do my game files live?",
  libraryNoteDesc:
    "Game files stay where they are on your Mac — Jisr runs them from there through a dedicated drive for that game only, keeping the rest of your Mac out of reach (the safe Z: replacement).",

  bottleWhatTitle: "What's a bottle?",
  bottleWhatDesc:
    "A bottle is a mini Windows environment inside your Mac where your games run by default: saves and settings live in one safe place. If a specific game needs full isolation, you can give it its own private bottle.",
  newBottle: "Private bottle",
  bottleMainName: "Main Bottle",
  bottlePrivateName: "Private Bottle",
  bottleSetupTitle: "Set up the Main Bottle",
  bottleSetup: "Set it up now",
  bottleSetupDesc:
    "One mini Windows environment, set up once (~1 min) — all your games run in it.",
  bottleNeedEngine: "Install the Wine engine first — from Home or Settings.",
  bottleSettingUp: "Setting up…",
  bottleSettingUpHint: "First setup takes a moment — keep the app open.",
  bottleReadyNow: "Ready to use",
  bottleFailedTitle: "Setup didn't finish",
  bottleRetry: "Try again",
  bottleDevSim: "Skip check (dev mode)",
  bottleDelete: "Delete",
  bottleDeleteWarn:
    "Sure? Everything inside this bottle will be erased — including saves.",
  bottleDeleteYes: "Delete permanently",
  bottleDeleteNo: "Cancel",

  appearance: "Appearance",
  themeLight: "Light",
  themeDark: "Dark",
  themeSystem: "System",
  language: "Language",
  languageSystem: "System language",
  graphicsLayer: "Graphics layer",
  font: "Font",
  fontSystem: "System font",
  fontDesc: "Jisr uses the system font (SF on Mac) so it feels native",
  autoSaveNote: "Settings save automatically the moment you change them — no save button.",
  engineSection: "Engine",
  install: "Install",
  saved: "Saved",
  comingSoon: "Coming soon",
  showInDock: "Show in advanced settings",
  advanced: "Advanced",

  logsTitle: "Logs & diagnostics",
  copyReport: "Copy report",
  copied: "Copied ✓",
  clearLogs: "Clear",
  logsEmpty: "Nothing logged yet — anything that happens shows up here.",

  // Downloads
  dlPreparing: "Preparing…",
  dlVerifying: "Verifying the file (SHA-256)…",
  dlExtracting: "Extracting & installing the engine…",
  filesSuffix: "files",
  dlDone: "Downloaded & verified ✓",
  dlReady: "Wine engine is ready ✓",
  dlFailed: "Download failed",
  dlInstallFailed: "Engine install failed",
  dlCancelled: "Download cancelled — you can resume anytime",
  dlCancel: "Cancel",

  soonEngine: "Engine install lands in the next step 🚧",
  soonGames: "Adding games lands in the next step 🚧",
  soonDevice: "Device scan needs to run on a Mac 🍎",
  soonBottles: "Bottle management lands in the next step 🚧",
  soonLogs: "Full diagnostics land in the next step 🚧",

  // Log events (Rust emits keys — we translate here)
  "log.startup": "Jisr started — {os}/{arch}",
  "log.scanDone": "Device scan complete",
  "log.engineFound": "Wine engine: {path}",
  "log.engineNotFound": "Wine engine is not installed",
  "log.rosettaReady": "Rosetta 2 is ready ✓",
  "log.rosettaMissing": "Rosetta 2 is not installed — required to run Windows games",
  "log.cleared": "Log cleared",
  "log.downloadStarted": "Downloading {name} {version} — {size}",
  "log.downloadDone": "Finished {name} {version} — checksum verified ✓",
  "log.downloadFailed": "Failed to download {name}: {error}",
  "log.downloadCancelled": "Cancelled {name} — partial file kept for resume",
  "log.extractStarted": "Extracting {name} {version}…",
  "log.extractDone": "Extracted {files} files ({size})",
  "log.extractFailed": "Failed to extract {name}: {error}",
  "log.extractSkipped": "Skipped {count} symlinks during extraction (outside the install folder or unsupported on Windows)",
  "log.engineReady": "Engine ready ✓ — {path}",
  "log.bottleStarted": "Setting up {name}…",
  "log.bottleReady": "{name} is ready ✓",
  "log.bottleFailed": "Failed to set up {name}: {error}",
  "log.bottleDeleted": "Deleted {name}",
  "log.gameAdded": "Added {name} to library",
  "log.gameRemoved": "Removed {name} from library",
  "log.gameLaunching": "Launching {name}…",
  "log.gameExited": "{name} ended — played {dur}",
  "log.gameStopped": "Stopped {name} — played {dur}",
  "log.gameLaunchFailed": "Failed to launch {name}: {error}",

  // Steam
  steamCardTitle: "Steam — Windows build",
  steamDesc:
    "Downloads Valve's official installer and sets it up inside your main bottle. Note: this is the Windows build of Steam — fully separate from your Mac Steam.",
  steamNeedBottle:
    "Set up the main bottle first (Bottles page) — then we can set up Steam.",
  steamStepDownload: "Step 1 — download the official Steam installer (Windows build)",
  steamStepDownloadHint: "2.3 MB from Valve's servers — checksum verified automatically.",
  steamDownloadBtn: "Download installer",
  steamInstallerReady: "Installer ready ✓ — install the Windows build into the bottle.",
  steamStepInstall: "Step 2 — install Steam (Windows build) into the bottle",
  steamStepInstallHint:
    "This is the Windows build of Steam — completely separate from Mac Steam. The installer window will open — follow the usual steps (Next, then Install). The first launch takes a bit while it updates.",
  steamInstallBtn: "Install Steam for Windows",
  steamInstalling: "Installing… finish the window",
  steamStepRun: "Steam (Windows) is ready to launch",
  steamRunBtn: "Launch Steam",
  steamRunningNow: "Steam is running",
  steamStop: "Stop",
  steamNote:
    "Inside Steam: sign in and download a game (try the free LIMBO Demo) — then play right from Steam, or add the game's .exe to Jisr for one-click launch from here.",
  steamAutoDl: "Downloading the official Steam installer automatically — a few seconds…",
  "log.steamInstalling": "Started installing Steam into the main bottle…",
  "log.steamInstalled": "Steam installed ✓ — ready to launch",
  "log.steamInstallExited": "The installer ended without Steam appearing — try again",
  "log.steamInstallFailed": "Steam install failed: {error}",
  "log.steamInstallStopped": "Stopped the Steam install",
  "log.steamLaunching": "Launching Steam…",
  "log.steamExited": "Steam exited",
  "log.steamStopped": "Stopped Steam",
  "log.steamLaunchFailed": "Failed to launch Steam: {error}",
};

export const dicts = { ar, en }; // قواميس اللغة (يستخدمها الاختبار والواجهة)

// ---------- الأنواع ----------
export type Lang = "ar" | "en";
export type LangChoice = "system" | "ar" | "en";

type I18nValue = {
  lang: Lang;
  dir: "rtl" | "ltr";
  choice: LangChoice;
  setChoice: (c: LangChoice) => void;
  t: (k: TKey, params?: Record<string, unknown>) => string;
};

const Ctx = createContext<I18nValue | null>(null);

/** تعويض المعاملات داخل النص: "بدأ {os}/{arch}" + {os:"mac"} → "بدأ mac/darwin" */
function interpolate(s: string, params?: Record<string, unknown>): string {
  if (!params) return s;
  return s.replace(/\{(\w+)\}/g, (_, k) =>
    params[k] != null ? String(params[k]) : `{${k}}`,
  );
}

// لغة النظام: عربي؟ → عربي، غير كذا إنجليزي
function detectLang(): Lang {
  const l = (navigator.language || "en").toLowerCase();
  return l.startsWith("ar") ? "ar" : "en";
}

function resolveLang(choice: LangChoice): Lang {
  return choice === "system" ? detectLang() : choice;
}

const STORAGE_KEY = "jisr.lang";

export function I18nProvider({ children }: { children: ReactNode }) {
  const [choice, setChoice] = useState<LangChoice>(
    () => (localStorage.getItem(STORAGE_KEY) as LangChoice | null) ?? "system",
  );

  const lang = resolveLang(choice);
  const dir: "rtl" | "ltr" = lang === "ar" ? "rtl" : "ltr";

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, choice);
    const el = document.documentElement;
    el.lang = lang;
    el.dir = dir;
    document.title = dicts[lang].appTitle;
  }, [choice, lang, dir]);

  const value = useMemo<I18nValue>(
    () => ({
      lang,
      dir,
      choice,
      setChoice,
      t: (k: TKey, params?: Record<string, unknown>) =>
        interpolate(dicts[lang][k] ?? k, params),
    }),
    [lang, dir, choice],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useI18n(): I18nValue {
  const v = useContext(Ctx);
  if (!v) throw new Error("useI18n لازم يُستخدم داخل I18nProvider");
  return v;
}
