// ============================================================
//  جسر — المظهر: فاتح / داكن / تلقائي (حسب النظام)
//  القرار محفوظ في localStorage، والألوان تُطبَّق على <html data-theme>
// ============================================================
import { useEffect, useState } from "react";

export type ThemeChoice = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "jisr.theme";

export function useTheme() {
  const [theme, setTheme] = useState<ThemeChoice>(
    () => (localStorage.getItem(STORAGE_KEY) as ThemeChoice | null) ?? "system",
  );

  const [systemDark, setSystemDark] = useState(
    () => window.matchMedia("(prefers-color-scheme: dark)").matches,
  );

  // نتابع تغيّر مظهر النظام (إذا المستخدم على "تلقائي")
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const resolved: ResolvedTheme =
    theme === "system" ? (systemDark ? "dark" : "light") : theme;

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, theme);
    const el = document.documentElement;
    el.dataset.theme = resolved;
    el.style.colorScheme = resolved; // يخلي عناصر النظام الأصلية متوافقة
  }, [theme, resolved]);

  return { theme, setTheme, resolved };
}
