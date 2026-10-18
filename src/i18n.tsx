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

  // القوارير
  bottlesEmptyTitle: "ما فيه قوارير بعد",
  bottlesEmptyDesc:
    "القارورة = بيئة ويندوز معزولة لكل لعبة، عشان ما تتعارض الألعاب مع بعض.",
  bottleWhatTitle: "شنو القارورة؟",
  bottleWhatDesc:
    "كل لعبة تشتغل داخل «قارورة» خاصة فيها: إعداداتها وخطوطها وملفاتها. تمسح القارورة = تمسح كل شي يخص اللعبة بدون ما تأثر على غيرها.",
  newBottle: "قارورة جديدة",

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

  // رسائل
  soonEngine: "تثبيت المحرك بيجي في الخطوة الجاية 🚧",
  soonGames: "إضافة الألعاب بيجي في الخطوة الجاية 🚧",
  soonDevice: "فحص الجهاز يحتاج نشتغل على ماك 🍎",
  soonBottles: "إدارة القوارير بيجي في الخطوة الجاية 🚧",
  soonLogs: "التشخيص الكامل بيجي في الخطوة الجاية 🚧",
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

  bottlesEmptyTitle: "No bottles yet",
  bottlesEmptyDesc:
    "A bottle is an isolated Windows environment per game, so games never clash.",
  bottleWhatTitle: "What's a bottle?",
  bottleWhatDesc:
    "Each game runs inside its own bottle: its own settings, fonts and files. Delete a bottle and everything for that game goes with it, leaving the others untouched.",
  newBottle: "New bottle",

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

  soonEngine: "Engine install lands in the next step 🚧",
  soonGames: "Adding games lands in the next step 🚧",
  soonDevice: "Device scan needs to run on a Mac 🍎",
  soonBottles: "Bottle management lands in the next step 🚧",
  soonLogs: "Full diagnostics land in the next step 🚧",
};

export const dicts = { ar, en };

// ---------- الأنواع ----------
export type Lang = "ar" | "en";
export type LangChoice = "system" | "ar" | "en";

type I18nValue = {
  lang: Lang;
  dir: "rtl" | "ltr";
  choice: LangChoice;
  setChoice: (c: LangChoice) => void;
  t: (k: TKey) => string;
};

const Ctx = createContext<I18nValue | null>(null);

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
      t: (k: TKey) => dicts[lang][k],
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
