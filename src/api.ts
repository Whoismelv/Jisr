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
  | "done"
  | "error"
  | "cancelled";

export type DownloadProgress = {
  id: string;
  received: number;
  total: number;
  speed_bps: number;
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
