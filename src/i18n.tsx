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
  preview: "نسخة تجريبية",
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
  graphicsDxmt: "طبقة الرسومات",
  gstreamerLabel: "GStreamer (متطلب Wine)",
  ready: "جاهز",
  installing: "قيد التثبيت…",
  notInstalled: "غير مثبت",
  yourLibrary: "مكتبتك",
  noGames: "فارغة — 0 لعبة",
  libraryGamesIn: "لعبة في مكتبتك",
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
  gfxAuto: "تلقائي (موصى به)",
  gfxHint:
    "DXVK يسرّع DirectX 10/11. لو ظهرت مشاكل في لعبة، بدّله إلى WineD3D. التغيير يحتاج إعادة تشغيل ستيم.",
  dxvkLayer: "طبقة DXVK",
  layersSection: "الطبقات (تسريع الألعاب)",
  autoPlan: "الطبقة المناسبة",
  autoNeedsDxvk: "تحتاج تثبيت طبقة DXVK قبل التشغيل",
  autoNeedsD3dx: "تحتاج تثبيت D3DX9 الأصلي (من صفحة الإعدادات)",
  layerInstalled: "مثبّتة وجاهزة ✓",
  dxvkMissingHint:
    "مو مثبّتة — بدونها تشتغل الألعاب على WineD3D البطيء وقد تنهار. التنزيل 5 MB.",
  d3dxLayer: "D3DX9 (مايكروسوفت)",
  cefShimLabel: "غلاف واجهة ستيم",
  cefShimHint:
    "إصلاح نافذة ستيم السوداء. عطّله لو ما ظهرت النافذة — بنرجع لستيم الأصلي (يحتاج إعادة تشغيل ستيم).",
  shimOn: "مفعّل",
  shimOff: "معطّل",
  d3dxHint:
    "يصلح ألعاب DirectX 9 القديمة اللي تنهار في d3dx9_43 (مثل LIMBO). تنزيل 96 MB من مايكروسوفت، ويُتحقق من بصمته.",
  font: "الخط",
  fontSystem: "خط النظام",
  fontDesc: "التطبيق يستخدم خط النظام (SF على الماك) عشان يطلع طبيعي",
  autoSaveNote: "الإعدادات تُحفظ تلقائياً بمجرد ما تغيّرها — بدون زر حفظ.",
  engineSection: "Engine",
  engineChoice: "نسخة المحرك",
  engineChoiceHint:
    "11.18 = الأحدث (فيها Vulkan/MoltenVK كامل). 11.6.1 = بديل فيه حزمة «إصلاح ستيم» من Gcenx — جرّبه لو واجهة ستيم ما ظهرت.",
  engineChoiceSaved: "بدّلنا نسخة المحرك — أعد تشغيل ستيم لتطبيقها.",
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
  copyFullLog: "انسخ السجل كامل",
  revealLog: "إظهار في Finder",
  logCopied: "نسخنا التقرير كامل — أرسله للمطوّر",
  logCopyFailed: "تعذّر النسخ — جرّب مرة ثانية",

  // التحديثات
  updateSection: "التحديثات",
  updateLabel: "نسخة جسر",
  updateAuto: "نتحقّق تلقائياً بعد الفتح",
  updateCheck: "تحقّق من التحديثات",
  updateChecking: "جارٍ التحقق…",
  updateAvailable: "نسخة جديدة متاحة: {v}",
  updateInstall: "ثبّتها الآن",
  updateDownloading: "جارٍ التنزيل…",
  updateInstalled: "تم التثبيت — أعد التشغيل لتطبيقها",
  updateRestart: "أعد التشغيل الآن",
  updateUpToDate: "نسخة جسر محدّثة — ما فيه جديد",
  updateFailed: "تعذّر التحديث — بنحاول لاحقاً",

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
  "log.raw": "{line}",
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
    "من داخل ستيم: سجّل دخولك ونزّل لعبتك (جرّب LIMBO Demo المجانية) — بعدها تقدر تشغّل لعبتك مباشرة من صفحة المكتبة بضغطة (قسم «ألعاب ستيم المثبّتة»)، أو تلعب من داخل ستيم نفسه.",
  steamAutoDl: "ننزّل مثبّت ستيم الرسمي تلقائياً — ياخذ ثواني…",
  steamLibraryTitle: "ألعاب ستيم المثبّتة",
  steamLibraryCount: "{n} لعبة",
  steamLibraryLoading: "نقرأ مكتبة ستيم…",
  steamLibraryEmpty:
    "ما لقينا ألعاباً مثبّتة — نزّل لعبة من داخل ستيم ثم اضغط «تحديث».",
  steamLibraryHint: "اضغط «تشغيل» — ستيم يفتح اللعبة مباشرة.",
  steamLibraryNeedRun:
    "شغّل ستيم أولاً من الأعلى، ثم تقدر تشغّل أي لعبة من هنا.",
  steamGameLaunched: "أرسلنا أمر تشغيل «{name}» إلى ستيم.",
  steamAppId: "رقم {id}",
  windowedBtn: "نافذة",
  windowedHint:
    "يشغّل اللعبة داخل نافذة بدل ما تاخذ شاشة الماك كاملة — يمنع الشاشة السوداء",
  windowedApplied:
    "فعّلنا وضع النافذة في «{name}» — شغّلها الآن من زر «تشغيل».",
  steamUiGpuOn: "رسم الواجهة: GPU",
  steamUiGpuOff: "رسم الواجهة: برمجي",
  cefModeSoftware: "برمجي",
  cefModeSingle: "أحادي",
  cefModeGpu: "GPU",
  steamUiGpuHint:
    "الرسم بالـGPU (عبر Vulkan) أوضح وأسرع — لازم تعيد تشغيل ستيم بعد التبديل",
  steamUiGpuSaved: "حفظنا الإعداد — أعد تشغيل ستيم لتطبيقه.",
  steamUiScale:
    "مقياس واجهة ستيم: {n}× — على شاشات Retina نستخدم 2× عشان تكون حادّة.",
  "issue.dx12":
    "DX12 — يحتاج D3DMetal من Apple (ما نوزّعه): غالباً ما تشتغل",
  "issue.dx8": "DX8 — مسار غير مدعوم بعد (يحتاج طبقة d8vk)",
  "issue.anticheat": "مضاد غش بدرايفر ويندوز — مستحيل على الماك",
  "issue.x86": "32-بت — ممكن تحتاج تجربة/إعدادات إضافية",
  "issue.dotnet": ".NET — قد تحتاج تثبيت .NET داخل القارورة",
  lastLaunch: "آخر تشغيل: {game} · {dx} · {arch}-بت —",
  layerOk: "الطبقة DXVK محمّلة ✓",
  layerNoDeploy: "ملفات DXVK ما انتشرت داخل القارورة ✗",
  layerNoRegistry: "تجاوزات الريجستري ما انكتبت ✗ (Wine يستخدم WineD3D)",
  layerNoDll: "ملف d3d9.dll ما اننسخ لنظام القارورة ✗",
  layerNoVulkan32: "Vulkan 32-بت غير موجود في المحرك ✗ (DXVK 32-بت ما يقدر يشتغل)",
  displayScaleHint:
    "لو الواجهة لسه ضبابية: اكتب 2 في الملف ~/Library/Application Support/Jisr/display-scale.txt وأعد تشغيل ستيم.",
  "log.graphicsReady": "تجهّزت طبقة DXVK لتسريع ألعاب DirectX 10/11.",
  "log.graphicsFailed":
    "تعذّر تجهيز DXVK — الألعاب ستشتغل بالطبقة الافتراضية: {error}",
  "log.steamGameLaunching": "أرسلنا أمر تشغيل لعبة ستيم ({appid}).",
  "log.steamGameLaunchFailed": "تعذّر تشغيل لعبة ستيم ({appid}): {error}",
  "log.steamGameWindowed": "فعّلنا وضع النافذة للعبة ({appid}).",
  "log.steamInstalling": "بدأنا تثبيت ستيم داخل القارورة الرئيسية…",
  "log.steamInstalled": "تم تثبيت ستيم ✓ — جاهز للتشغيل",
  "log.steamInstallExited": "المثبّت انتهى بدون ما يظهر ستيم — جرّب مرة ثانية",
  "log.steamInstallFailed": "فشل تثبيت ستيم: {error}",
  "log.steamInstallStopped": "أوقفت تثبيت ستيم",
  "log.steamLaunching": "جاري تشغيل ستيم…",
  "log.steamUiFix": "ثبّتنا إصلاح واجهة ستيم (غلاف CEF) ✓",
  "log.steamUiFixFailed": "تعذّر تجهيز إصلاح واجهة ستيم — قد تفتح الواجهة سوداء",
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
  preview: "Preview",
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
  graphicsDxmt: "Graphics layer",
  gstreamerLabel: "GStreamer (Wine prerequisite)",
  ready: "Ready",
  installing: "Installing…",
  notInstalled: "Not installed",
  yourLibrary: "Your library",
  noGames: "Empty — 0 games",
  libraryGamesIn: "games in your library",
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
  gfxAuto: "Auto (recommended)",
  gfxHint:
    "DXVK speeds up DirectX 10/11. If a game misbehaves, switch to WineD3D. Changing it requires restarting Steam.",
  dxvkLayer: "DXVK layer",
  layersSection: "Layers (game acceleration)",
  autoPlan: "Right layer",
  autoNeedsDxvk: "Needs the DXVK layer installed before playing",
  autoNeedsD3dx: "Needs native D3DX9 (install it from Settings)",
  layerInstalled: "Installed and ready ✓",
  dxvkMissingHint:
    "Not installed — without it games fall back to slow WineD3D and may crash. 5 MB download.",
  d3dxLayer: "D3DX9 (Microsoft)",
  cefShimLabel: "Steam UI wrapper",
  cefShimHint:
    "Fixes the black Steam window. Turn it off if no window appears — we restore Valve's original (restart Steam after).",
  shimOn: "On",
  shimOff: "Off",
  d3dxHint:
    "Fixes old DirectX 9 games that crash in d3dx9_43 (e.g. LIMBO). 96 MB from Microsoft, signature verified.",
  font: "Font",
  fontSystem: "System font",
  fontDesc: "Jisr uses the system font (SF on Mac) so it feels native",
  autoSaveNote: "Settings save automatically the moment you change them — no save button.",
  engineSection: "Engine",
  engineChoice: "Engine version",
  engineChoiceHint:
    "11.18 = newest (full Vulkan/MoltenVK). 11.6.1 = alternative with Gcenx's Steam fix — try it if the Steam window never shows.",
  engineChoiceSaved: "Engine version changed — restart Steam to apply it.",
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
  copyFullLog: "Copy full log",
  revealLog: "Show in Finder",
  logCopied: "Full report copied — send it to the developer",
  logCopyFailed: "Copy failed — try again",

  // Updates
  updateSection: "Updates",
  updateLabel: "Jisr version",
  updateAuto: "We check automatically after launch",
  updateCheck: "Check for updates",
  updateChecking: "Checking…",
  updateAvailable: "New version available: {v}",
  updateInstall: "Install now",
  updateDownloading: "Downloading…",
  updateInstalled: "Installed — restart to apply it",
  updateRestart: "Restart now",
  updateUpToDate: "Jisr is up to date",
  updateFailed: "Update failed — we'll retry later",

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
  "log.raw": "{line}",
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
    "Inside Steam: sign in and download a game (try the free LIMBO Demo) — then launch it in one click from the Library page (Installed Steam games), or play right inside Steam.",
  steamAutoDl: "Downloading the official Steam installer automatically — a few seconds…",
  steamLibraryTitle: "Installed Steam games",
  steamLibraryCount: "{n} games",
  steamLibraryLoading: "Reading your Steam library…",
  steamLibraryEmpty:
    "No installed games found — download a game inside Steam, then press Refresh.",
  steamLibraryHint: "Press Play — Steam launches the game directly.",
  steamLibraryNeedRun:
    "Start Steam above first, then you can launch any game from here.",
  steamGameLaunched: "Asked Steam to launch “{name}”.",
  steamAppId: "App {id}",
  windowedBtn: "Windowed",
  windowedHint:
    "Runs the game inside a window instead of taking over the whole screen — prevents a black screen",
  windowedApplied:
    "Windowed mode is on for “{name}” — launch it now with Play.",
  steamUiGpuOn: "UI rendering: GPU",
  steamUiGpuOff: "UI rendering: software",
  cefModeSoftware: "Software",
  cefModeSingle: "Single",
  cefModeGpu: "GPU",
  steamUiGpuHint:
    "GPU rendering (via Vulkan) is sharper and faster — restart Steam after switching",
  steamUiGpuSaved: "Saved — restart Steam to apply it.",
  steamUiScale:
    "Steam UI scale: {n}× — we use 2× on Retina displays so it stays sharp.",
  "issue.dx12":
    "DX12 — needs Apple D3DMetal (we can't ship it): usually won't run",
  "issue.dx8": "DX8 — not supported yet (needs a d8vk layer)",
  "issue.anticheat": "Kernel-level anti-cheat — impossible on macOS",
  "issue.x86": "32-bit — may need extra setup/testing",
  "issue.dotnet": ".NET — may need .NET installed inside the bottle",
  lastLaunch: "Last launch: {game} · {dx} · {arch}-bit —",
  layerOk: "DXVK layer loaded ✓",
  layerNoDeploy: "DXVK files were not deployed into the bottle ✗",
  layerNoRegistry: "Registry overrides were not written ✗ (Wine falls back to WineD3D)",
  layerNoDll: "d3d9.dll was not copied into the bottle's system dir ✗",
  layerNoVulkan32: "32-bit Vulkan is missing from the engine ✗ (32-bit DXVK can't load)",
  displayScaleHint:
    "Still blurry? Write 2 into ~/Library/Application Support/Jisr/display-scale.txt and restart Steam.",
  "log.graphicsReady": "DXVK is ready for DirectX 10/11 game acceleration.",
  "log.graphicsFailed":
    "DXVK setup failed — games will use the default layer: {error}",
  "log.steamGameLaunching": "Asked Steam to launch app {appid}.",
  "log.steamGameLaunchFailed": "Couldn't launch Steam app {appid}: {error}",
  "log.steamGameWindowed": "Enabled windowed mode for app {appid}.",
  "log.steamInstalling": "Started installing Steam into the main bottle…",
  "log.steamInstalled": "Steam installed ✓ — ready to launch",
  "log.steamInstallExited": "The installer ended without Steam appearing — try again",
  "log.steamInstallFailed": "Steam install failed: {error}",
  "log.steamInstallStopped": "Stopped the Steam install",
  "log.steamLaunching": "Launching Steam…",
  "log.steamUiFix": "Installed the Steam UI fix (CEF shim) ✓",
  "log.steamUiFixFailed": "Could not prepare the Steam UI fix — the window may open black",
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
