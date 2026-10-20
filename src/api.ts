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
};

/** لعبة ستيم مثبّتة داخل القارورة (نقرأها محلياً من appmanifest) */
export type SteamGame = {
  appid: number;
  name: string;
  size_bytes: number;
  probe: GameProbe;
  plan: GamePlan;
};

/** كل ألعاب ستيم المثبّتة — قراءة محلية فقط */
export const steamGamesList = () => call<SteamGame[]>("steam_games_list");

/** تشغيل لعبة عبر Steam (ستيم لازم يكون شغّالاً) */
export const steamGameLaunch = (appid: number) =>
  call<void>("steam_game_launch", { appid });

/** تفعيل/تعطيل وضع النافذة للعبة (settings.txt) — يمنع الشاشة السوداء */
export const steamGameWindowed = (appid: number, enabled: boolean) =>
  call<{ applied: boolean; file?: string }>("steam_game_windowed", {
    appid,
    enabled,
  });

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

/** المحرّك المستخدم حالياً (11.18 أو البديل 11.6.1 لواجهة ستيم) */
export const getEngineChoice = () => call<string>("engine_choice");
export const setEngineChoice = (id: string) => call<string>("engine_set_choice", { id });
export const getEngineChoices = () => call<string[]>("engine_choices");

/** حالة D3DX9 الأصلي (نسخة مايكروسوفت) — إصلاح ألعاب DirectX 9 */export type D3dxStatus = { installed: boolean; deployed: boolean };
export const d3dxStatus = () => call<D3dxStatus>("d3dx_status");

/** غلاف CEF مفعّل؟ (مخرج طوارئ لو واجهة ستيم ما ظهرت) */
export const cefShimStatus = () => call<boolean>("cef_shim_status");
export const cefShimEnable = () => call<void>("cef_shim_enable");
export const cefShimDisable = () => call<boolean>("cef_shim_disable");
