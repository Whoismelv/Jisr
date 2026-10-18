// ============================================================
//  جسر — السجل المباشر (يجيب القديم + يسمع الجديد لحظياً)
// ============================================================
import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { clearLogs, getLogs, type LogEntry } from "./api";

const MAX_IN_UI = 500;

export function useLogs() {
  const [logs, setLogs] = useState<LogEntry[]>([]);

  useEffect(() => {
    let alive = true;

    // السجل المخزّن
    getLogs()
      .then((list) => {
        if (alive) setLogs(list);
      })
      .catch(() => {});

    // الجديد لحظياً
    const unlisten = listen<LogEntry>("jisr-log", (event) => {
      setLogs((prev) => [...prev.slice(-(MAX_IN_UI - 1)), event.payload]);
    });

    return () => {
      alive = false;
      unlisten.then((off) => off());
    };
  }, []);

  const clear = useCallback(async () => {
    try {
      await clearLogs();
    } catch {
      /* تجاهل */
    }
  }, []);

  return { logs, clear };
}
