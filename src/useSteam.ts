// ============================================================
//  جسر — خطاف ستيم
//  • يجيب الحالة من الخلفية + يستمع لأحداث ستيم وأحداث تنزيل المثبّت
//  • التثبيت/التشغيل/الإيقاف أوامر قصيرة + إشعار عند أي خطأ
//  • عند اكتمال التثبيت → إشعار لطيف تلقائياً
// ============================================================
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "./i18n";
import {
  steamInstall,
  steamRun,
  steamStatus,
  steamStop,
  type SteamStatus,
} from "./api";

/** معرّف مثبّت ستيم في كتالوج التنزيل */
export const STEAM_SETUP_ID = "steam-setup";

export type Steam = {
  status: SteamStatus | null;
  install: () => void;
  run: () => void;
  stop: () => void;
  refresh: () => void;
};

export function useSteam(notify: (m: string) => void): Steam {
  const { t } = useI18n();
  const [status, setStatus] = useState<SteamStatus | null>(null);
  const installedRef = useRef<boolean | null>(null);

  const refresh = useCallback(() => {
    steamStatus()
      .then((s) => {
        // خلص التثبيت من أول لحظة؟ → إشعار لطيف
        if (installedRef.current === false && s.installed) {
          notify(t("log.steamInstalled"));
        }
        installedRef.current = s.installed;
        setStatus(s);
      })
      .catch(() => {});
  }, [notify, t]);

  useEffect(() => {
    refresh();
    const unSteam = listen("jisr-steam", () => refresh());
    // تنزيل مثبّت ستيم (بدأ/تحقق/خلص/فشل) → نحدّث الحالة
    const unDl = listen<{ id: string }>("jisr-download", (e) => {
      if (e.payload.id === STEAM_SETUP_ID) refresh();
    });
    // تجهزت القارورة الرئيسية للتو؟ → نعيد الفحص (بعده يشتغل التحقق التلقائي)
    const unBottles = listen("jisr-bottles", () => refresh());
    return () => {
      unSteam.then((off) => off());
      unDl.then((off) => off());
      unBottles.then((off) => off());
    };
  }, [refresh]);

  const wrap = useCallback(
    (fn: () => Promise<void>) => {
      fn()
        .then(refresh)
        .catch((err) => {
          // رسالة الخلفية عربية وواضحة (مو جاهز / التثبيت شغّال / ...)
          notify(String(err));
          refresh();
        });
    },
    [notify, refresh],
  );

  const install = useCallback(() => wrap(steamInstall), [wrap]);
  const run = useCallback(() => wrap(steamRun), [wrap]);
  const stop = useCallback(() => wrap(steamStop), [wrap]);

  // هوية ثابتة للكائن ⇒ بطاقات ستيم ما تُعاد رسمها إلا عند تغيّر فعلي
  return useMemo(
    () => ({ status, install, run, stop, refresh }),
    [status, install, run, stop, refresh],
  );
}
