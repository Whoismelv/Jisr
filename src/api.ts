// ============================================================
//  جسر — جسر التواصل مع الخلفية (Rust) بشكل مكتوب الأنواع
// ============================================================
import { invoke } from "@tauri-apps/api/core";

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
  is_apple_silicon: boolean;
  chip: string;
  cpu_cores: number | null;
  memory_mb: number | null;
  gpus: GpuInfo[];
  rosetta_installed: boolean;
  engine_installed: boolean;
  engine_path: string | null;
  graphics_layer: string | null;
};

export type LogEntry = {
  time_ms: number;
  level: string;
  key: string;
  params?: Record<string, unknown>;
};

/** فحص الجهاز (نظام، معالج، ذاكرة، رسومات، Rosetta، المحرك) */
export const getSystemInfo = () => invoke<SystemInfo>("system_info");

/** كل السجل المخزّن */
export const getLogs = () => invoke<LogEntry[]>("get_logs");

/** تفريغ السجل */
export const clearLogs = () => invoke<void>("clear_logs");

/** نكتب سطر في السجل من الواجهة (بمفتاح + معاملات) */
export const logLine = (level: string, key: string, params?: Record<string, unknown>) =>
  invoke<void>("log_line", { level, key, params: params ?? {} });

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
export const getEngineCatalog = () => invoke<EngineAsset[]>("download_catalog");

/** ابدأ تنزيل عنصر من الكتالوج (id مثل "wine-11.18") */
export const startDownload = (id: string) =>
  invoke<void>("download_start", { id });

/** ألغِ التنزيل — الجزء المنزّل يُحفظ ويُستكمل لاحقاً */
export const cancelDownload = (id: string) =>
  invoke<void>("download_cancel", { id });

/** آخر حالة معروفة لكل تنزيل (لما تُفتح الواجهة من جديد) */
export const downloadsSnapshot = () =>
  invoke<DownloadProgress[]>("downloads_snapshot");

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
export const bottlesList = () => invoke<Bottle[]>("bottles_list");

/** إنشاء قارورة — kind: "main" | "private" (simulate للتطوير على ويندوز فقط) */
export const bottleCreate = (kind: "main" | "private", simulate = false) =>
  invoke<void>("bottle_create", { name: null, kind, simulate });

/** إعادة محاولة تجهيز قارورة فشلت */
export const bottleRetry = (id: string, simulate = false) =>
  invoke<void>("bottle_retry", { id, simulate });

/** حذف قارورة خاصة (الرئيسية محميّة في الخلفية) */
export const bottleDelete = (id: string) => invoke<void>("bottle_delete", { id });

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
export const gamesList = () => invoke<Game[]>("games_list");

/** إضافة لعبة من مسار .exe (يجي من نافذة الاختيار الأصلية) */
export const gameAdd = (exe: string) => invoke<Game>("game_add", { exe });

/** حذف لعبة من المكتبة (ملفاتها ما تُمس) */
export const gameRemove = (id: string) => invoke<void>("game_remove", { id });

/** تشغيل لعبة داخل قارورتها */
export const gameLaunch = (id: string) => invoke<void>("game_launch", { id });

/** إيقاف لعبة شغّالة */
export const gameStop = (id: string) => invoke<void>("game_stop", { id });

// ============================================================
//  ستيم (المسار الرسمي)
// ============================================================
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
};

/** حالة ستيم الكاملة */
export const steamStatus = () => invoke<SteamStatus>("steam_status");

/** ثبّت ستيم داخل القارورة الرئيسية بالمثبّت الرسمي */
export const steamInstall = () => invoke<void>("steam_install");

/** شغّل ستيم */
export const steamRun = () => invoke<void>("steam_run");

/** أوقف ستيم (أو التثبيت الجاري) */
export const steamStop = () => invoke<void>("steam_stop");

/** لعبة ستيم مثبّتة داخل القارورة (نقرأها محلياً من appmanifest) */
export type SteamGame = {
  appid: number;
  name: string;
  size_bytes: number;
};

/** كل ألعاب ستيم المثبّتة — قراءة محلية فقط */
export const steamGamesList = () => invoke<SteamGame[]>("steam_games_list");

/** تشغيل لعبة عبر Steam (ستيم لازم يكون شغّالاً) */
export const steamGameLaunch = (appid: number) =>
  invoke<void>("steam_game_launch", { appid });

// ============================================================
//  طبقة الرسومات
// ============================================================
export type GraphicsMode = "auto" | "dxvk" | "wined3d";

/** الوضع الحالي لطبقة الرسومات (auto | dxvk | wined3d) */
export const getGraphicsMode = () => invoke<GraphicsMode>("graphics_mode");

/** يغيّر طبقة الرسومات ويُطبّقها على التشغيل التالي */
export const setGraphicsMode = (mode: GraphicsMode) =>
  invoke<void>("graphics_set_mode", { mode });
