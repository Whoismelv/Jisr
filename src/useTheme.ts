// ============================================================
//  جسر — المظهر: فاتح / داكن / تلقائي (حسب النظام)
//  • حالة واحدة على مستوى التطبيق كله ⇒ مستمع نظام واحد فقط
//  • تُطبَّق فور تحميل الملف ⇒ بدون وميض عند فتح التطبيق
//  • أي تغيير يُحفظ تلقائياً (بدون زر حفظ — أسلوب آبل)
// ============================================================
import { useCallback, useSyncExternalStore } from "react";

export type ThemeChoice = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "jisr.theme";

// ---------- الحالة المشتركة (نسخة واحدة للبرنامج كله) ----------
let choice: ThemeChoice =
  (localStorage.getItem(STORAGE_KEY) as ThemeChoice | null) ?? "system";

const media = window.matchMedia("(prefers-color-scheme: dark)");
let systemDark = media.matches;

let snapshot = { choice, systemDark };
const listeners = new Set<() => void>();

function apply() {
  const resolved: ResolvedTheme =
    choice === "system" ? (systemDark ? "dark" : "light") : choice;
  const el = document.documentElement;
  el.dataset.theme = resolved;
  el.style.colorScheme = resolved; // يخلي عناصر النظام الأصلية (تمرير، تحديد…) متوافقة
}

function emit() {
  snapshot = { choice, systemDark };
  for (const l of listeners) l();
}

// نطبّق فوراً — قبل أول رسم — حتى ما يطلع وميض أبيض
apply();

// مستمع واحد لتغيّر مظهر النظام (طول عمر التطبيق)
media.addEventListener("change", (e) => {
  systemDark = e.matches;
  apply();
  emit();
});

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

export function useTheme() {
  const snap = useSyncExternalStore(subscribe, () => snapshot);

  const setTheme = useCallback((next: ThemeChoice) => {
    choice = next;
    localStorage.setItem(STORAGE_KEY, next); // حفظ تلقائي فوري
    apply();
    emit();
  }, []);

  const resolved: ResolvedTheme =
    snap.choice === "system"
      ? snap.systemDark
        ? "dark"
        : "light"
      : snap.choice;

  return { theme: snap.choice, setTheme, resolved };
}
