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
