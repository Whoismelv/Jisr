// ============================================================
//  جسر — التحديث التلقائي (موقّع)
//  • يفحص عن نسخة جديدة بعد الإقلاع (بدون إزعاج) وعند الطلب من الإعدادات
//  • التوقيع: minisign — المفتاح العمومي داخل tauri.conf.json،
//    فما يقدر يجيب تحديث إلا من بناء CI عندنا (ولا حتى لو أحد عدّل الـlatest.json)
//  • بعد التثبيت: إعادة تشغيل التطبيق لتطبيق النسخة الجديدة
// ============================================================
import { useCallback, useEffect, useRef, useState } from "react";
import { check as checkForUpdate, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateStatus =
  | "idle"
  | "checking"
  | "upToDate"
  | "available"
  | "downloading"
  | "ready"
  | "error";

export type Updates = {
  status: UpdateStatus;
  version: string | null;
  error: string | null;
  progress: number | null;
  /** فحص يدوي (من زر الإعدادات) */
  check: () => void;
  /** تنزيل + تثبيت */
  install: () => void;
  /** إعادة تشغيل التطبيق لتطبيق النسخة الجديدة */
  restart: () => void;
};

export function useUpdates(): Updates {
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [version, setVersion] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  // كائن التحديث: لازم نخزّنه (لازم يبقى حيّ حتى التثبيت)
  const pending = useRef<Update | null>(null);

  const doCheck = useCallback(async (silent: boolean) => {
    setStatus("checking");
    setError(null);
    try {
      const update = await checkForUpdate();
      if (!update) {
        pending.current = null;
        setVersion(null);
        setStatus("upToDate");
        return;
      }
      pending.current = update;
      setVersion(update.version);
      setStatus("available");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      setStatus("error");
      if (!silent) {
        // نخلي الواجهة تعرض الخطأ — ما نرمي إشعار مزعج عند الفحص التلقائي
        console.warn("update check failed:", msg);
      }
    }
  }, []);

  // فحص تلقائي بعد الإقلاع — لو فيه نسخة جديدة بتظهر في الإعدادات
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void doCheck(true);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [doCheck]);

  const check = useCallback(() => {
    void doCheck(false);
  }, [doCheck]);

  const install = useCallback(() => {
    const update = pending.current;
    if (!update) return;
    setStatus("downloading");
    setProgress(0);
    let downloaded = 0;
    let total = 0;
    update
      .downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setProgress(
            total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null,
          );
        } else {
          setProgress(100);
        }
      })
      .then(() => {
        setStatus("ready");
        pending.current = null;
      })
      .catch((e) => {
        const msg = e instanceof Error ? e.message : String(e);
        setError(msg);
        setStatus("error");
      });
  }, []);

  const restart = useCallback(() => {
    void relaunch();
  }, []);

  return { status, version, error, progress, check, install, restart };
}
