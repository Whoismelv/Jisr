// ============================================================
//  جسر — السجل المباشر (يجيب القديم + يسمع الجديد لحظياً)
// ============================================================
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { clearLogs, getLogs, type LogEntry } from "./api";

const MAX_IN_UI = 500;

export function useLogs() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const pending = useRef<LogEntry[]>([]); // دفعة الأسطر المنتظرة
  const frame = useRef(0); // رقم إطار الرسم القادم

  useEffect(() => {
    let alive = true;

    // السجل المخزّن
    getLogs()
      .then((list) => {
        if (alive) setLogs(list);
      })
      .catch(() => {});

    // الجديد لحظياً — نجمع الأسطر وندفعها مرة وحدة كل إطار رسم،
    // حتى لو وصلت مئات الأسطر بثانية الواجهة ترسم مرة وحدة فقط (سلاسة كاملة)
    const flush = () => {
      frame.current = 0;
      const batch = pending.current;
      if (batch.length === 0) return;
      pending.current = [];
      setLogs((prev) => [...prev, ...batch].slice(-MAX_IN_UI));
    };

    const unlisten = listen<LogEntry>("jisr-log", (event) => {
      const buf = pending.current;
      buf.push(event.payload);
      if (buf.length > 700) buf.splice(0, buf.length - 600); // حماية لو النافذة مخفية
      if (!frame.current) frame.current = requestAnimationFrame(flush);
    });

    return () => {
      alive = false;
      if (frame.current) cancelAnimationFrame(frame.current);
      frame.current = 0;
      pending.current = [];
      unlisten.then((off) => off());
    };
  }, []);

  const clear = useCallback(async () => {
    pending.current = [];
    setLogs([]); // نفضّي الواجهة فوراً (استجابة فورية)، والخلفية تضيف سطر "تم التفريغ"
    try {
      await clearLogs();
    } catch {
      /* تجاهل */
    }
  }, []);

  // هوية ثابتة للكائن ⇒ الصفحة ما تُعاد رسمها بلا داعي
  return useMemo(() => ({ logs, clear }), [logs, clear]);
}
