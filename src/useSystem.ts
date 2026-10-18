// ============================================================
//  جسر — فحص الجهاز (يشتغل أول ما يفتح التطبيق + يقدر يعيد الفحص)
// ============================================================
import { useCallback, useEffect, useState } from "react";
import { getSystemInfo, type SystemInfo } from "./api";

export function useSystem() {
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [scanning, setScanning] = useState(false);

  const scan = useCallback(async () => {
    setScanning(true);
    try {
      setInfo(await getSystemInfo());
    } catch {
      // لو فشل الفحص نخلي الواجهة تعرض "غير متوفر"
    } finally {
      setScanning(false);
    }
  }, []);

  // فحص تلقائي عند فتح التطبيق (مثل برامج آبل: المعلومات جاهزة بدون ما تطلب)
  useEffect(() => {
    void scan();
  }, [scan]);

  return { info, scanning, scan };
}
