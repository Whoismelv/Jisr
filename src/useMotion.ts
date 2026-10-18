// ============================================================
//  جسر — الحركة: كاملة / مخففة / تلقائي (حسب النظام)
//  المستخدم يقدر يتجاوز تفضيل النظام من داخل البرنامج.
//  • نضبط data-motion على <html> ⇒ قواعد CSS تحترمه
//  • وMotionConfig في الواجهة يقرأ منه ⇒ نفس القرار للمكوّنات
// ============================================================
import { useCallback, useSyncExternalStore } from "react";

export type MotionChoice = "system" | "full" | "reduced";

const STORAGE_KEY = "jisr.motion";

// ---------- الحالة المشتركة ----------
let choice: MotionChoice =
  (localStorage.getItem(STORAGE_KEY) as MotionChoice | null) ?? "system";

const media = window.matchMedia("(prefers-reduced-motion: reduce)");
let systemReduced = media.matches;

let snapshot = { choice, systemReduced };
const listeners = new Set<() => void>();

export function isReduced(): boolean {
  return choice === "system" ? systemReduced : choice === "reduced";
}

function apply() {
  document.documentElement.dataset.motion = isReduced() ? "reduced" : "full";
}

function emit() {
  snapshot = { choice, systemReduced };
  for (const l of listeners) l();
}

// يُطبَّق فور تحميل الوحدة — قبل أول رسم
apply();

media.addEventListener("change", (e) => {
  systemReduced = e.matches;
  apply();
  emit();
});

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

export function useMotion() {
  const snap = useSyncExternalStore(subscribe, () => snapshot);

  const setMotion = useCallback((next: MotionChoice) => {
    choice = next;
    localStorage.setItem(STORAGE_KEY, next); // حفظ تلقائي فوري
    apply();
    emit();
  }, []);

  const reduced =
    snap.choice === "system" ? snap.systemReduced : snap.choice === "reduced";

  return { motion: snap.choice, setMotion, reduced };
}
