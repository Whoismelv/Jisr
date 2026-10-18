// ============================================================
//  جسر — خطاف القوارير
//  • يحمّل القائمة عند الفتح + يستمع لحدث "jisr-bottles" من الخلفية
//  • العمليات (إنشاء/إعادة/حذف) تُنعش القائمة تلقائياً بعدها
// ============================================================
import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  bottleCreate,
  bottleDelete,
  bottleRetry,
  bottlesList,
  type Bottle,
} from "./api";

export type Bottles = {
  items: Bottle[];
  create: (kind: "main" | "private") => void;
  retry: (id: string, simulate?: boolean) => void;
  remove: (id: string) => void;
  refresh: () => void;
};

export function useBottles(): Bottles {
  const [items, setItems] = useState<Bottle[]>([]);

  const refresh = useCallback(() => {
    bottlesList()
      .then(setItems)
      .catch(() => {});
  }, []);

  // أول تحميل + متابعة حية: أي تجهيز يخلص بالخلفية يوصلنا حدث
  useEffect(() => {
    refresh();
    const un = listen("jisr-bottles", () => refresh());
    return () => {
      un.then((off) => off());
    };
  }, [refresh]);

  const create = useCallback(
    (kind: "main" | "private") => {
      bottleCreate(kind)
        .then(refresh)
        .catch(refresh); // حتى لو رفضت الخلفية (مثلاً الرئيسية موجودة) ننعش الحالة
    },
    [refresh],
  );

  const retry = useCallback(
    (id: string, simulate = false) => {
      bottleRetry(id, simulate)
        .then(refresh)
        .catch(refresh);
    },
    [refresh],
  );

  const remove = useCallback(
    (id: string) => {
      bottleDelete(id)
        .then(refresh)
        .catch(refresh);
    },
    [refresh],
  );

  // هوية ثابتة للكائن ⇒ صفحات المكتبة والقوارير ما تُعاد رسمها بلا داعي
  return useMemo(
    () => ({ items, create, retry, remove, refresh }),
    [items, create, retry, remove, refresh],
  );
}
