// ============================================================
//  جسر — جسر التواصل مع الخلفية (Rust) بشكل مكتوب الأنواع
// ============================================================
import { invoke } from "@tauri-apps/api/core";

/**
 * كل نداء للخلفية يمر من هنا: أي فشل يُكتب في السجل الدائم باسم الأمر —
 * حتى لو الواجهة ما عرضته، يبقى أثر التشخيص موجود للتقرير.
 */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    const msg =
      typeof e === "string"
        ? e
        : e instanceof Error
          ? (e.stack ?? e.message)
          : JSON.stringify(e);
    // invoke مباشرة (لا call) حتى لا ندخل في حلقة لو فشل التسجيل نفسه
    void invoke<void>("debug_log", {
      level: "error",
      message: `invoke ${cmd} failed: ${msg}`,
    }).catch(() => {});
    throw e;
  }
}

export type GpuInfo = {
  name: string;
  vendor: string;
  cores: number | null;
  vram_mb: number | null;
};

export type SystemInfo = {
  os_name: string;
  os_version: string;
  arch: string;
  app_version: string;
  is_apple_silicon: boolean;
  chip: string;
  cpu_cores: number | null;
  memory_mb: number | null;
  gpus: GpuInfo[];
  rosetta_installed: boolean;
  engine_installed: boolean;
  engine_path: string | null;
  graphics_layer: string | null;
  /** طبقة DXVK مثبّتة فعلاً (ملفاتها موجودة) */
  dxvk_installed: boolean;
  gstreamer_installed: boolean;
  gstreamer_version: string | null;
};

export type LogEntry = {
  time_ms: number;
  level: string;
  key: string;
  params?: Record<string, unknown>;
};

/** فحص الجهاز (نظام، معالج، ذاكرة، رسومات، Rosetta، المحرك) */
export const getSystemInfo = () => call<SystemInfo>("system_info");

/** كل السجل المخزّن */
export const getLogs = () => call<LogEntry[]>("get_logs");

/** تفريغ السجل */
export const clearLogs = () => call<void>("clear_logs");

/** نكتب سطر في السجل من الواجهة (بمفتاح + معاملات) */
export const logLine = (level: string, key: string, params?: Record<string, unknown>) =>
  call<void>("log_line", { level, key, params: params ?? {} });

/** سطر تشخيص خام من الواجهة (أخطاء JS…) — يظهر حرفياً في السجل الدائم */
export const debugLog = (
  level: "debug" | "info" | "warn" | "error",
  message: string,
) => call<void>("debug_log", { level, message }).catch(() => {});

/** مسار ملف السجل الدائم (لعرضه في Finder) */
export const debugLogPath = () => call<string | null>("debug_log_path");

/** تقرير تشخيص كامل: الجهاز + السجل الدائم + سجلات ستيم والألعاب — جاهز للنسخ */
export const exportDebugReport = () => call<string>("export_debug_report");

// ============================================================
//  محرّك التنزيل
// ============================================================
export type EngineAsset = {
  id: string;
  name: string;
  version: string;
  kind: string;
  url: string;
  sha256: string;
  size: number;
};

export type DownloadPhase =
  | "starting"
  | "downloading"
  | "verifying"
  | "extracting"
  | "done"
  | "error"
  | "cancelled";

export type DownloadProgress = {
  id: string;
  received: number;
  total: number;
  speed_bps: number;
  /** عدد الملفات المفكوكة (مرحلة الفك والتركيب) */
  files: number;
  /** أي مرحلة كانت آخر شي: download | install (لرسالة الخطأ) */
  op: string;
  phase: DownloadPhase;
  error: string | null;
};

/** كتالوج المحركات الرسمي (إصدارات وبصمات موثّقة من المصدر) */
export const getEngineCatalog = () => call<EngineAsset[]>("download_catalog");

/** ابدأ تنزيل عنصر من الكتالوج (id مثل "wine-11.18") */
export const startDownload = (id: string) =>
  call<void>("download_start", { id });

/** ألغِ التنزيل — الجزء المنزّل يُحفظ ويُستكمل لاحقاً */
export const cancelDownload = (id: string) =>
  call<void>("download_cancel", { id });

/** آخر حالة معروفة لكل تنزيل (لما تُفتح الواجهة من جديد) */
export const downloadsSnapshot = () =>
  call<DownloadProgress[]>("downloads_snapshot");

// ============================================================
//  القوارير
// ============================================================
export type Bottle = {
  id: string;
  name: string;
  /** main | private */
  kind: string;
  /** creating | ready | error */
  state: string;
  error: string | null;
  path: string;
  size_bytes: number;
  created_ms: number;
};

/** كل القوارير مع أحجامها */
export const bottlesList = () => call<Bottle[]>("bottles_list");

/** إنشاء قارورة — kind: "main" | "private" (simulate للتطوير على ويندوز فقط) */
export const bottleCreate = (kind: "main" | "private", simulate = false) =>
  call<void>("bottle_create", { name: null, kind, simulate });

/** إعادة محاولة تجهيز قارورة فشلت */
export const bottleRetry = (id: string, simulate = false) =>
  call<void>("bottle_retry", { id, simulate });

/** حذف قارورة خاصة (الرئيسية محميّة في الخلفية) */
export const bottleDelete = (id: string) => call<void>("bottle_delete", { id });

// ============================================================
//  الألعاب
// ============================================================
export type Game = {
  id: string;
  name: string;
  /** exe | steam (ستيم: مرحلة لاحقة) */
  kind: string;
  exe: string;
  dir: string;
  bottle: string;
  played_ms: number;
  added_ms: number;
  running: boolean;
};

/** كل الألعاب مع حالة التشغيل */
export const gamesList = () => call<Game[]>("games_list");

/** إضافة لعبة من مسار .exe (يجي من نافذة الاختيار الأصلية) */
export const gameAdd = (exe: string) => call<Game>("game_add", { exe });

/** حذف لعبة من المكتبة (ملفاتها ما تُمس) */
export const gameRemove = (id: string) => call<void>("game_remove", { id });

/** تشغيل لعبة داخل قارورتها */
export const gameLaunch = (id: string) => call<void>("game_launch", { id });

/** إيقاف لعبة شغّالة */
export const gameStop = (id: string) => call<void>("game_stop", { id });

// ============================================================
//  ستيم (المسار الرسمي)
// ============================================================
/** حقائق آخر تشغيل لعبة: هل الطبقة انتشرت فعلاً؟ */
export type LastLaunch = {
  at_ms: number;
  game: string;
  dx: string;
  is_64: boolean;
  dxvk_deployed: boolean;
  registry_ok: boolean;
  d3d9_64: boolean;
  d3d9_32: boolean;
  vulkan_32: boolean;
};

export type SteamStatus = {
  /** مثبّت ستيم الرسمي منزّل ومحفوظ؟ */
  installer_ready: boolean;
  /** ستيم مثبّت داخل القارورة الرئيسية؟ */
  installed: boolean;
  /** مسار تشغيل ستيم داخل القارورة (ويندوزي) */
  steam_exe: string | null;
  installing: boolean;
  running: boolean;
  bottle_ready: boolean;
  /** رسم واجهة ستيم بالـGPU (أحدّ) بدل الرسم البرمجي */
  ui_gpu: boolean;
  /** وضع رسم الواجهة: software | single | gpu */
  ui_mode: CefMode;
  /** معامل تحجيم الواجهة: 2 على شاشات Retina */
  desktop_scale: number;
  /** آخر لعبة شغّلناها وحالة الطبقة وقتها */
  last_launch: LastLaunch | null;
};

/** حالة ستيم الكاملة */
export const steamStatus = () => call<SteamStatus>("steam_status");

/** تبديل رسم واجهة ستيم (GPU ↔ برمجي) — لازم تعيد تشغيل ستيم */
export const steamUiGpu = (enabled: boolean) =>
  call<boolean>("steam_ui_gpu", { enabled });

/** ثبّت ستيم داخل القارورة الرئيسية بالمثبّت الرسمي */
export const steamInstall = () => call<void>("steam_install");

/** شغّل ستيم */
export const steamRun = () => call<void>("steam_run");

/** أوقف ستيم (أو التثبيت الجاري) */
export const steamStop = () => call<void>("steam_stop");

/** فحص اللعبة من ملفها التنفيذي (DirectX + المعمارية + موانع) */
export type GameProbe = {
  exe: string | null;
  /** d3d9 · d3d11 · d3d12 · vulkan · opengl · none */
  dx: string | null;
  is_64: boolean | null;
  /** dx8 · dx12 · anticheat · x86 · dotnet */
  issues: string[];
  /** اللعبة تحتاج d3dx9_*.dll (نسخة Wine تنهار) */
  needs_d3dx9: boolean;
  /** اللعبة تحتاج d3dcompiler_*.dll */
  needs_d3dcompiler: boolean;
};

/** خطة التشغيل التلقائي: الطبقات المناسبة + الموانع + الخطوات */
export type GamePlan = {
  layers: string[];
  layer_labels: string[];
  blockers: string[];
  notes: string[];
  actions: string[];
  launchable: boolean;
  /** التركيبة الفعلية (ملف التعريف) */
  profile_ar: string;
  profile_attempts: number;
};

/** لعبة ستيم مثبّتة داخل القارورة (نقرأها محلياً من appmanifest) */
export type SteamGame = {
  appid: number;
  name: string;
  size_bytes: number;
  probe: GameProbe;
  plan: GamePlan;
  /** اختيار المستخدم: ملء الشاشة أو نافذة (محفوظ لكل لعبة) */
  display: "fullscreen" | "windowed";
  /** اللعبة تقبل تغيير windowedmode من settings.txt؟ */
  display_supported: boolean;
};

/** كل ألعاب ستيم المثبّتة — قراءة محلية فقط */
export const steamGamesList = () => call<SteamGame[]>("steam_games_list");

/** تشغيل لعبة عبر Steam (ستيم لازم يكون شغّالاً) */
export const steamGameLaunch = (appid: number) =>
  call<void>("steam_game_launch", { appid });

/** اختيار طريقة العرض (ملء الشاشة/نافذة) — يُحفظ ويُطبّق فوراً */
export type DisplayResult = {
  /** انكتب في settings.txt الآن؟ */
  applied: boolean;
  /** اللعبة تدعم المفتاح؟ لو لا، الاختيار محفوظ لكن اللعبة تحكم بنفسها */
  supported: boolean;
  file?: string;
};
export const steamGameDisplaySet = (
  appid: number,
  display: "fullscreen" | "windowed",
) => call<DisplayResult>("steam_game_display_set", { appid, display });

/** توافق قديم: تفعيل/تعطيل وضع النافذة */
export const steamGameWindowed = (appid: number, enabled: boolean) =>
  call<{ applied: boolean; file?: string }>("steam_game_windowed", {
    appid,
    enabled,
  });

/** متطلبات ويندوز: Visual C++ 2015‑2022 (x86 + x64) */
export type VcredistStatus = {
  installed: boolean;
  x86: boolean;
  x64: boolean;
  version: string;
  missing: string[];
};
export const vcredistStatus = () => call<VcredistStatus>("vcredist_status");
export const vcredistPrepare = () => call<VcredistStatus>("vcredist_prepare");

// ============================================================
//  طبقة الرسومات
// ============================================================
export type GraphicsMode = "auto" | "dxvk" | "wined3d";

/** الوضع الحالي لطبقة الرسومات (auto | dxvk | wined3d) */
export const getGraphicsMode = () => call<GraphicsMode>("graphics_mode");

/** يغيّر طبقة الرسومات ويُطبّقها على التشغيل التالي */
export const setGraphicsMode = (mode: GraphicsMode) =>
  call<void>("graphics_set_mode", { mode });

/** وضع رسم واجهة ستيم: software | single | gpu */
export type CefMode = "software" | "single" | "gpu";
export const getCefMode = () => call<CefMode>("cef_mode");
export const setCefMode = (mode: CefMode) => call<CefMode>("cef_set_mode", { mode });

// ============================================================
//  محرّك الرسوميات لكل قارورة
// ============================================================
export type GraphicsBackendId =
  | "auto"
  | "dxvk"
  | "wined3d"
  | "dxmt"
  | "d3dmetal";

/** وصف محرك رسم واحد (هل هو متاح على هذا الجهاز؟) */
export type GraphicsBackendInfo = {
  id: GraphicsBackendId;
  available: boolean;
  note: string;
};

/** كل المحركات المتاحة للاختيار */
export const getGraphicsBackends = () =>
  call<GraphicsBackendInfo[]>("graphics_backends");

/** محرّك الرسوميات المختار لهذه القارورة */
export const getGraphicsBackend = (bottle: string) =>
  call<string>("graphics_backend_get", { bottle });

/** يغيّر محرّك الرسوميات للقارورة (يُطبَّق على التشغيل التالي) */
export const setGraphicsBackend = (bottle: string, backend: string) =>
  call<string>("graphics_backend_set", { bottle, backend });

/** هل D3DMetal (Game Porting Toolkit من Apple) مثبّت على الجهاز؟ */
export const d3dmetalPresent = () => call<boolean>("d3dmetal_present");

// ============================================================
//  قاعدة بروفايلات التوافق (محلية بالكامل)
// ============================================================
export type ProfileSummary = {
  appid: string;
  name: string;
  category: string;
  medal: string | null;
  rating: number | null;
  bottle_template: string | null;
  dependencies: string[];
  is_application: boolean;
};

export type CompatStats = {
  profiles: number;
  files: number;
  ratings: number;
};

/** بحث في قاعدة البروفايلات المحلية */
export const compatSearch = (query: string, lang: string, limit: number) =>
  call<ProfileSummary[]>("compat_search", { query, lang, limit });

/** بروفايل واحد بمعرّفه */
export const compatGet = (appid: string, lang: string) =>
  call<ProfileSummary | null>("compat_get", { appid, lang });

/** عدّادات قاعدة البروفايلات */
export const compatStats = () => call<CompatStats>("compat_stats");

/** يستورد ملف بروفايلات (.c4p) إلى مخزننا — يتحقق قبل النسخ */
export const compatImport = (path: string) =>
  call<CompatStats>("compat_import", { path });

/** يصدّر قاعدة البروفايلات المدمجة كملف c4p (للمشاركة) */
export const compatExport = (path: string) =>
  call<string>("compat_export", { path });

/** اسم ملف التصدير الافتراضي (في مجلد المستندات) */
export const defaultExportName = () =>
  "~/Documents/jisr-profiles.c4p";

/** قاعدة توافق واحدة (كما تُعرض في المحرر) */
export type RuleInfo = {
  index: number;
  matcher: string;
  hacks: string[];
};

/** كل قواعدنا المحلية */
export const rulesList = () => call<RuleInfo[]>("rules_list");

/** يحفظ قاعدة (يتحقق من صحة XML قبل الحفظ) */
export const rulesSave = (name: string, xml: string) =>
  call<void>("rules_save", { name, xml });

/** مشكلة مكتشفة في القارورة + طريقة حلّها */
export type Issue = {
  id: string;
  level: "info" | "warning" | "error";
  title: string;
  description: string;
  fix: string | null;
  command: string | null;
};

/** يفحص القارورة ويعرض المشاكل */
export const fixesCheck = (bottle: string) => call<Issue[]>("fixes_check", { bottle });

/** يشغّل/يوقف msync للقارورة */
export const fixesSetMsync = (bottle: string, enabled: boolean) =>
  call<boolean>("fixes_set_msync", { bottle, enabled });

/** يبني اختصارات macOS (.app) لكل تطبيقات القارورة */
export const menuSyncBottle = (bottle: string) =>
  call<string[]>("menu_sync_bottle", { bottle });

/** يثبّت مساعد القارورة (تطبيق macOS يلتقط Apple Events) */
export const helperInstall = (bottle: string) =>
  call<string>("helper_install", { bottle });

/** ملف تعريف اللعبة: التركيبة التي نجحت معها (طبقات + محرك رسم + overlay) */export type GameProfile = {
  appid: number;
  d3d9: "Dxvk" | "WineD3D";
  d3dx9: "Native" | "Builtin";
  compiler: "Native" | "Builtin";
  renderer: "Vulkan" | "Gl";
  /** "fullscreen" أو "windowed" */
  display: "fullscreen" | "windowed";
  windowed: boolean;
  overlay: boolean;
  attempts: number;
  updated_ms: number;
};
export const getGameProfile = (appid: number) =>
  call<GameProfile>("steam_game_profile", { appid });
export const resetGameProfile = (appid: number) =>
  call<void>("steam_game_profile_reset", { appid });

/** انهيار مُصنَّف (يحدده التطبيق تلقائياً من تقرير Wine) */
export type CrashState = {
  at_ms: number;
  appid: number;
  class: string;
  headline_ar: string;
  next_ar: string;
  action: string;
  attempts: number;
};
export const getCrashStates = () => call<CrashState[]>("steam_crash_states");
export const getAutoRetry = () => call<boolean>("steam_auto_retry_status");
export const setAutoRetry = (enabled: boolean) =>
  call<boolean>("steam_auto_retry", { enabled });

/** المحرّك المستخدم حالياً (11.18 أو البديل 11.6.1 لواجهة ستيم) */
export const getEngineChoice = () => call<string>("engine_choice");
export const setEngineChoice = (id: string) => call<string>("engine_set_choice", { id });
export const getEngineChoices = () => call<string[]>("engine_choices");

/** محرّك خارجي جاهز عند المستخدم (مثل Whisky) — نكتشفه ونستخدمه كما هو */
export const detectExternalEngines = () => call<string[]>("engine_detect_external");
export const externalEngineStatus = () => call<string | null>("engine_external_status");
export const setExternalEngine = (path: string) =>
  call<string>("engine_set_external", { path });
export const clearExternalEngine = () => call<void>("engine_clear_external");

/** غلاف تشخيص مترجم الـHLSL (D3DX shim) */
export type D3dxShimStatus = {
  /** المستخدم فعّل الحماية (علامة دائمة داخل القارورة) */
  enabled: boolean;
  /** الملف الحالي هو غلافنا فعلاً */
  installed: boolean;
  log: string;
  tail: string;
};
export const installD3dxShim = () => call<boolean>("d3dx_shim_install");
export const removeD3dxShim = () => call<boolean>("d3dx_shim_remove");
export const getD3dxShimStatus = () => call<D3dxShimStatus>("d3dx_shim_status");

/** حالة D3DX9 الأصلي (نسخة مايكروسوفت) — إصلاح ألعاب DirectX 9 */export type D3dxStatus = { installed: boolean; deployed: boolean };
export const d3dxStatus = () => call<D3dxStatus>("d3dx_status");

/** غلاف CEF مفعّل؟ (مخرج طوارئ لو واجهة ستيم ما ظهرت) */
export const cefShimStatus = () => call<boolean>("cef_shim_status");
export const cefShimEnable = () => call<void>("cef_shim_enable");
export const cefShimDisable = () => call<boolean>("cef_shim_disable");
