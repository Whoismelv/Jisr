// ============================================================
//  جسر — خطاف الألعاب
//  • تحمّل المكتبة عند الفتح + تستمع لحدث "jisr-games" (أي تغيير)
//  • "jisr-game-exit" يجيب نهاية اللعبة → تنبيه لطيف بالمدة/السبب
//  • الإضافة: نفتح نافذة اختيار الملف الأصلية (.exe) ثم نحفظها في الخلفية
// ============================================================
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "./i18n";
import {
  gameAdd,
  gameLaunch,
  gameRemove,
  gameStop,
  gamesList,
  type Game,
} from "./api";

type GameExit = {
  id: string;
  name: string;
  code: number | null;
  played_ms: number;
  stopped: boolean;
  quick_fail: boolean;
};

export type Games = {
  items: Game[];
  add: () => void;
  remove: (id: string) => void;
  launch: (id: string) => void;
  stop: (id: string) => void;
  refresh: () => void;
};

/** مدة لطيفة: 12:34 أو 1:02:03 */
export function fmtPlayed(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const p = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${p(m)}:${p(sec)}` : `${m}:${p(sec)}`;
}

export function useGames(notify: (m: string) => void): Games {
  const { t } = useI18n();
  const [items, setItems] = useState<Game[]>([]);
  const busy = useRef(false);

  const refresh = useCallback(() => {
    gamesList()
      .then(setItems)
      .catch(() => {});
  }, []);

  // أول تحميل + متابعة حية
  useEffect(() => {
    refresh();
    const unGames = listen("jisr-games", () => refresh());
    const unExit = listen<GameExit>("jisr-game-exit", (e) => {
      const p = e.payload;
      if (p.stopped) {
        notify(t("gameStopToast", { name: p.name }));
      } else if (p.quick_fail) {
        notify(t("gameFailedToast", { name: p.name }));
      } else {
        notify(t("gameExitToast", { name: p.name, dur: fmtPlayed(p.played_ms) }));
      }
      refresh();
    });
    return () => {
      unGames.then((off) => off());
      unExit.then((off) => off());
    };
  }, [notify, refresh, t]);

  // إضافة لعبة: نافذة اختيار أصلية بفلتر .exe (مع «كل الملفات» احتياطاً)
  const add = useCallback(() => {
    if (busy.current) return;
    busy.current = true;
    open({
      multiple: false,
      directory: false,
      title: t("addGame"),
      filters: [
        { name: t("exeFiles"), extensions: ["exe"] },
        { name: t("allFiles"), extensions: ["*"] },
      ],
    })
      .then((picked) => {
        if (typeof picked !== "string" || !picked) return;
        return gameAdd(picked).then((g) => {
          notify(t("gameAddedToast", { name: g.name }));
          refresh();
        });
      })
      .catch((err) => {
        // رسالة الخلفية عربية وواضحة (مو ملف exe / موجودة مسبقاً / ...)
        notify(String(err));
        refresh();
      })
      .finally(() => {
        busy.current = false;
      });
  }, [notify, refresh, t]);

  const remove = useCallback(
    (id: string) => {
      gameRemove(id)
        .then(refresh)
        .catch((err) => {
          notify(String(err));
          refresh();
        });
    },
    [notify, refresh],
  );

  const launch = useCallback(
    (id: string) => {
      gameLaunch(id)
        .then(refresh)
        .catch((err) => {
          notify(String(err));
          refresh();
        });
    },
    [notify, refresh],
  );

  const stop = useCallback(
    (id: string) => {
      gameStop(id)
        .then(refresh)
        .catch((err) => {
          notify(String(err));
          refresh();
        });
    },
    [notify, refresh],
  );

  // هوية ثابتة للكائن ⇒ بطاقات المكتبة ما تُعاد رسمها إلا عند تغيّر فعلي
  return useMemo(
    () => ({ items, add, remove, launch, stop, refresh }),
    [items, add, remove, launch, stop, refresh],
  );
}
