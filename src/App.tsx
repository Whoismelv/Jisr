// ============================================================
//  جسر — الهيكل الرئيسي (بأسلوب تطبيقات macOS)
//  • شريط جانبي شفاف + شريط أدوات بمواد آبل، تبديل صفحات بنوابض
//  • لغة تلقائية (عربي/إنجليزي) ومظهر (فاتح/داكن/تلقائي)
//  • الحركة: كاملة دائماً (نوابض آبل) — بلا إعداد يتغيّر
//  • الإعدادات تُحفظ تلقائياً + تذكّر آخر صفحة + اختصار ⌘,
// ============================================================
import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { useI18n, type TKey } from "./i18n";
import { useTheme } from "./useTheme";
import { useSystem } from "./useSystem";
import { useLogs } from "./useLogs";
import {
  useDownloads,
  ENGINE_ID,
  engineButtonState,
  isActive,
} from "./useDownloads";
import { useBottles } from "./useBottles";
import { useGames } from "./useGames";
import { useSteam } from "./useSteam";
import { Button, SPRINGS, type IconComp } from "./ui";
import {
  HomePage,
  LibraryPage,
  BottlesPage,
  SettingsPage,
  LogsPage,
  type Page,
} from "./pages";
import * as Icon from "./Icons";
import "./App.css";

const NAV: { id: Page; labelKey: TKey; icon: IconComp }[] = [
  { id: "home", labelKey: "navHome", icon: Icon.Home },
  { id: "library", labelKey: "navLibrary", icon: Icon.Gamepad },
  { id: "bottles", labelKey: "navBottles", icon: Icon.Bottle },
  { id: "settings", labelKey: "navSettings", icon: Icon.Sliders },
  { id: "logs", labelKey: "navLogs", icon: Icon.Doc },
];

const LAST_PAGE_KEY = "jisr.page";

export default function App() {
  const { t } = useI18n();
  useTheme(); // الحالة مشتركة — استدعاؤها يضمن تطبيق المظهر من أول لحظة

  const { info: sys, scanning, scan } = useSystem(); // فحص حقيقي يبدأ تلقائياً
  const { logs, clear } = useLogs(); // السجل المباشر
  const dl = useDownloads(); // محرّك التنزيل (يُدار في Rust)
  const bottles = useBottles(); // القوارير (تُدار في Rust)

  // نرجعك لآخر صفحة كنت فيها (وأول مرة: الرئيسية)
  const [page, setPage] = useState<Page>(() => {
    const saved = localStorage.getItem(LAST_PAGE_KEY);
    return NAV.some((n) => n.id === saved) ? (saved as Page) : "home";
  });
  useEffect(() => {
    localStorage.setItem(LAST_PAGE_KEY, page);
  }, [page]);

  // إشعار صغير يختفي لحاله (بأسلوب آبل)
  const [toast, setToast] = useState<{ id: number; msg: string } | null>(null);
  const notify = useCallback(
    (msg: string) => setToast({ id: Date.now(), msg }),
    [],
  );
  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => setToast(null), 2400);
    return () => window.clearTimeout(id);
  }, [toast]);

  const games = useGames(notify); // مكتبة الألعاب (تحتاج الإشعار — بعد تعريفه)
  const steam = useSteam(notify); // ستيم (المثبّت الرسمي + التشغيل)

  // نافذة الماك: فئة على <html> تضبط أضواء النافذة فوق المحتوى
  const [isMac] = useState(() => /Mac|iPhone|iPad/.test(navigator.userAgent));
  useEffect(() => {
    document.documentElement.classList.toggle("is-macos", isMac);
  }, [isMac]);

  // ⌘, يفتح الإعدادات (الاختصار القياسي في تطبيقات الماك)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        setPage("settings");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const onScan = useCallback(() => void scan(), [scan]);
  const onClear = useCallback(() => void clear(), [clear]);

  // تقرير التشخيص: معلومات حقيقية من الجهاز
  const copyReport = useCallback(async () => {
    const lines = [
      `${t("appName")} — ${t("preview")}`,
      `time: ${new Date().toISOString()}`,
      `os: ${[sys?.os_name, sys?.os_version].filter(Boolean).join(" ") || "-"}`,
      `arch: ${sys?.arch ?? "-"}`,
      `chip: ${sys?.chip ?? "-"}${
        sys?.cpu_cores ? ` (${sys.cpu_cores} ${t("coresSuffix")})` : ""
      }`,
      `memory: ${
        sys?.memory_mb ? `${Math.round(sys.memory_mb / 1024)} GB` : "-"
      }`,
      `gpu: ${sys?.gpus.map((g) => g.name).join(" / ") || "-"}`,
      `rosetta: ${
        sys ? (sys.rosetta_installed ? "installed" : "missing") : "-"
      }`,
      `engine: ${
        sys
          ? sys.engine_installed
            ? (sys.engine_path ?? "installed")
            : "missing"
          : "-"
      }`,
      `graphics: ${sys?.graphics_layer ?? "-"}`,
    ];
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
    } catch {
      /* الحافظة غير متاحة — نكمل على أي حال */
    }
    notify(t("copied"));
  }, [notify, sys, t]);

  const current = NAV.find((n) => n.id === page)!;

  // حالة تنزيل المحرك (لشارة الشريط الجانبي + الأزرار)
  const engineDl = dl.items[ENGINE_ID];
  const engineBtn = engineButtonState(engineDl, !!sys?.engine_installed);

  // لما تخلص سلسلة التنزيل+التركيب: إشعار + إعادة فحص تلقائي
  const prevEnginePhase = useRef<string | null>(null);
  useEffect(() => {
    const phase = engineDl?.phase ?? null;
    if (
      phase === "done" &&
      prevEnginePhase.current !== "done" &&
      !sys?.engine_installed
    ) {
      // لو صار فك ضغط ضمن السلسلة → المحرك جاهز فعلاً
      notify((engineDl?.files ?? 0) > 0 ? t("dlReady") : t("dlDone"));
      void scan();
    }
    prevEnginePhase.current = phase;
  }, [engineDl?.phase, engineDl?.files, notify, scan, sys?.engine_installed, t]);

  // ستيم: تحقق تلقائي — القارورة الرئيسية جاهزة وستيم مو مثبّت والمثبّت مو
  // منزّل؟ ننزّله تلقائياً مرة وحدة (وإذا ألغاه أو فشل — الزر اليدوي باقي)
  const steamAuto = useRef(false);
  useEffect(() => {
    const s = steam.status;
    if (!s || steamAuto.current) return;
    const sp = dl.items["steam-setup"];
    if (
      s.bottle_ready &&
      !s.installed &&
      !s.installer_ready &&
      !isActive(sp) &&
      sp?.phase !== "error" &&
      sp?.phase !== "cancelled"
    ) {
      steamAuto.current = true;
      notify(t("steamAutoDl"));
      dl.start("steam-setup");
    }
  }, [steam.status, dl, notify, t]);

  return (
    // الحركة كاملة دائماً — بلا استثناءات وبلا إعداد يتغيّر (قرار التصميم)
    <MotionConfig reducedMotion="never">
      <div className="app">
        {/* ===== الشريط الجانبي ===== */}
        <aside className="sidebar">
          <div className="brand">
            <span className="brand-mark">
              <Icon.Bridge size={19} />
            </span>
            <div className="brand-text">
              <span className="brand-name">{t("appName")}</span>
              <span className="brand-tag">{t("tagline")}</span>
            </div>
          </div>

          <nav className="nav">
            {NAV.map((item) => {
              const active = page === item.id;
              const IconC = item.icon;
              return (
                <button
                  key={item.id}
                  className={"nav-item" + (active ? " active" : "")}
                  onClick={() => setPage(item.id)}
                  aria-current={active ? "page" : undefined}
                >
                  {active && (
                    <motion.span
                      layoutId="nav-pill"
                      className="nav-pill"
                      transition={SPRINGS.fast}
                    />
                  )}
                  <span className="nav-icon">
                    <IconC size={17} />
                  </span>
                  <span className="nav-label">{t(item.labelKey)}</span>
                </button>
              );
            })}
          </nav>

          <div className="sidebar-foot">
            <div className="engine-pill">
              <span
                className={
                  "dot " +
                  (sys?.engine_installed
                    ? "ok"
                    : isActive(engineDl)
                      ? "busy"
                      : "off")
                }
              />
              <span>
                {sys?.engine_installed
                  ? t("ready")
                  : isActive(engineDl) && engineDl && engineDl.total > 0
                    ? `${Math.round((engineDl.received / engineDl.total) * 100)}%`
                    : t("notInstalled")}
              </span>
            </div>
            <span className="foot-note">
              {t("preview")} · {t("openSource")}
            </span>
          </div>
        </aside>

        {/* ===== المحتوى ===== */}
        <main className="main">
          <header className="toolbar" data-tauri-drag-region>
            <h1 className="toolbar-title" data-tauri-drag-region>
              {t(current.labelKey)}
            </h1>

            <div className="toolbar-actions">
              {page === "home" && (
                <>
                  <Button
                    variant="plain"
                    icon={<Icon.Plus size={15} />}
                    onClick={() => setPage("library")}
                  >
                    {t("addGame")}
                  </Button>
                  <Button
                    variant="filled"
                    icon={<Icon.Download size={15} />}
                    disabled={engineBtn !== "idle"}
                    onClick={() => dl.start(ENGINE_ID)}
                  >
                    {engineBtn === "busy"
                      ? t("installing")
                      : engineBtn === "done"
                        ? t("dlDone")
                        : engineBtn === "ready"
                          ? t("ready")
                          : t("installEngine")}
                  </Button>
                </>
              )}

              {page === "library" && (
                <Button
                  variant="filled"
                  icon={<Icon.Plus size={15} />}
                  onClick={() => games.add()}
                >
                  {t("addGame")}
                </Button>
              )}

              {page === "bottles" && (
                <Button
                  variant="filled"
                  icon={<Icon.Plus size={15} />}
                  onClick={() => bottles.create("private")}
                >
                  {t("newBottle")}
                </Button>
              )}

              {page === "logs" && (
                <>
                  <Button
                    variant="plain"
                    icon={<Icon.Trash size={15} />}
                    onClick={onClear}
                  >
                    {t("clearLogs")}
                  </Button>
                  <Button
                    variant="filled"
                    icon={<Icon.Copy size={15} />}
                    onClick={copyReport}
                  >
                    {t("copyReport")}
                  </Button>
                </>
              )}
            </div>
          </header>

          <div className="scroll">
            <AnimatePresence mode="wait" initial={false}>
              <motion.div
                key={page}
                className="page"
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                // خروج سريع جداً ⇒ إحساس فوري بالسويس (أسلوب آبل)
                exit={{ opacity: 0, y: -5, transition: { duration: 0.1 } }}
                transition={SPRINGS.page}
              >
                {page === "home" && (
                  <HomePage
                    go={setPage}
                    sys={sys}
                    scanning={scanning}
                    onScan={onScan}
                    dl={dl}
                  />
                )}
                {page === "library" && (
                  <LibraryPage games={games} bottles={bottles} steam={steam} dl={dl} />
                )}
                {page === "bottles" && <BottlesPage bottles={bottles} sys={sys} />}
                {page === "settings" && <SettingsPage sys={sys} dl={dl} />}
                {page === "logs" && <LogsPage logs={logs} onClear={onClear} />}
              </motion.div>
            </AnimatePresence>
          </div>
        </main>

        {/* ===== الإشعار ===== */}
        <div className="toast-wrap">
          <AnimatePresence>
            {toast && (
              <motion.div
                key={toast.id}
                className="toast"
                initial={{ opacity: 0, y: 14, scale: 0.96 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, y: 10, scale: 0.98 }}
                transition={SPRINGS.toast}
              >
                {toast.msg}
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>
    </MotionConfig>
  );
}
