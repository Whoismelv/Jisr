// ============================================================
//  جسر — التنزيلات (تُدار في Rust، والواجهة تسمع وتنفّذ الأوامر)
//  • snapshot عند الفتح + أحداث لحظية
//  • أدوات عرض: حجم/سرعة مقروءة، وهل التنزيل شغّال؟
// ============================================================
import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  cancelDownload,
  downloadsSnapshot,
  startDownload,
  type DownloadProgress,
} from "./api";

/** معرّف محرك Wine في الكتالوج الرسمي */
export const ENGINE_ID = "wine-11.18";

export type Downloads = {
  items: Record<string, DownloadProgress>;
  start: (id: string) => void;
  cancel: (id: string) => void;
};

/** هل التنزيل وسط العمل (بداية/تنزيل/تحقق/فك ضغط)؟ */
export function isActive(p?: DownloadProgress | null): boolean {
  return (
    !!p &&
    (p.phase === "starting" ||
      p.phase === "downloading" ||
      p.phase === "verifying" ||
      p.phase === "extracting")
  );
}

/** حالة زر «ثبّت المحرك»: شغّال / خلص / جاهز */
export type EngineBtn = "busy" | "done" | "ready" | "idle";
export function engineButtonState(
  p: DownloadProgress | undefined,
  installed: boolean,
): EngineBtn {
  if (installed) return "ready";
  if (isActive(p)) return "busy";
  if (p?.phase === "done") return "done";
  return "idle";
}

export function useDownloads(): Downloads {
  const [items, setItems] = useState<Record<string, DownloadProgress>>({});

  useEffect(() => {
    let alive = true;

    // آخر حالة معروفة (لو كان في تنزيل شغّال قبل ما نفتح الواجهة)
    downloadsSnapshot()
      .then((list) => {
        if (alive && list.length > 0)
          setItems((prev) => ({
            ...Object.fromEntries(list.map((p) => [p.id, p])),
            ...prev, // الأحدث من الأحداث يفوز
          }));
      })
      .catch(() => {});

    // التحديثات اللحظية
    const unlisten = listen<DownloadProgress>("jisr-download", (e) =>
      setItems((prev) => ({ ...prev, [e.payload.id]: e.payload })),
    );

    return () => {
      alive = false;
      unlisten.then((off) => off());
    };
  }, []);

  const start = useCallback((id: string) => {
    startDownload(id).catch(() => {});
  }, []);

  const cancel = useCallback((id: string) => {
    cancelDownload(id).catch(() => {});
  }, []);

  return { items, start, cancel };
}

/** حجم بصيغة مقروءة: 190.9 MB / 1.2 GB */
export function fmtBytes(n: number): string {
  if (!n || n <= 0) return "0 KB";
  let v = n / 1024;
  const units = ["KB", "MB", "GB"];
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

/** سرعة بصيغة مقروءة: 8.4 MB/s */
export function fmtSpeed(bps: number): string {
  return bps > 0 ? `${fmtBytes(bps)}/s` : "";
}
