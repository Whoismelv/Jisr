import { useEffect, useRef, useState, type ComponentType } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useI18n, type TKey } from "./i18n";
import { useTheme } from "./useTheme";
import { useSystem } from "./useSystem";
import { useLogs } from "./useLogs";
import type { LogEntry, SystemInfo } from "./api";
import * as Icon from "./Icons";
import type { IconProps } from "./Icons";
import "./App.css";

// ============================================================
//  جسر — الهيكل الرئيسي (بأسلوب تطبيقات macOS)
//  • لغة تلقائية (عربي/إنجليزي) + تبديل يدوي من الإعدادات
//  • مظهر: فاتح / داكن / تلقائي (يتبع النظام)
//  • حركة نوابض قابلة للمقاطعة (Motion) وتقلّل نفسها لو المستخدم يفضّل تقليل الحركة
// ============================================================

type Page = "home" | "library" | "bottles" | "settings" | "logs";
type IconComp = ComponentType<IconProps>;

const NAV: { id: Page; labelKey: TKey; icon: IconComp }[] = [
  { id: "home", labelKey: "navHome", icon: Icon.Home },
  { id: "library", labelKey: "navLibrary", icon: Icon.Gamepad },
  { id: "bottles", labelKey: "navBottles", icon: Icon.Bottle },
  { id: "settings", labelKey: "navSettings", icon: Icon.Sliders },
  { id: "logs", labelKey: "navLogs", icon: Icon.Doc },
];

export default function App() {
  const { t } = useI18n();
  const reduce = useReducedMotion();
  const [page, setPage] = useState<Page>("home");
  const [toast, setToast] = useState<{ id: number; msg: string } | null>(null);
  const [isMac] = useState(() => /Mac|iPhone|iPad/.test(navigator.userAgent));
  const { info: sys, scanning, scan } = useSystem(); // فحص الجهاز الحقيقي
  const { logs, clear } = useLogs(); // السجل المباشر

  // إشعار صغير يختفي لحاله (بأسلوب آبل)
  const notify = (msg: string) => setToast({ id: Date.now(), msg });
  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => setToast(null), 2400);
    return () => window.clearTimeout(id);
  }, [toast]);

  useEffect(() => {
    document.documentElement.classList.toggle("is-macos", isMac);
  }, [isMac]);

  // نوابض: بدون ارتداد افتراضياً + استجابة قصيرة (دamping = 1 في مفهوم آبل)
  const spring = reduce
    ? { duration: 0 }
    : { type: "spring" as const, stiffness: 520, damping: 38, mass: 0.9 };
  const pageSpring = reduce
    ? { duration: 0.18 }
    : { type: "spring" as const, stiffness: 430, damping: 40 };

  const titleKey = NAV.find((n) => n.id === page)!.labelKey;

  const copyReport = async () => {
    const report = [
      "Jisr diagnostics",
      "----------------",
      `platform: ${navigator.platform}`,
      `userAgent: ${navigator.userAgent}`,
      `language: ${navigator.language}`,
      `time: ${new Date().toISOString()}`,
      "engine: not installed",
      "graphics: DXMT (planned)",
    ].join("\n");
    try {
      await navigator.clipboard.writeText(report);
      notify(t("copied"));
    } catch {
      notify(t("copied"));
    }
  };

  return (
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
                    transition={spring}
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
            <span className="dot off" />
            <span>{t("notInstalled")}</span>
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
            {t(titleKey)}
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
                  onClick={() => notify(t("soonEngine"))}
                >
                  {t("installEngine")}
                </Button>
              </>
            )}

            {page === "library" && (
              <Button
                variant="filled"
                icon={<Icon.Plus size={15} />}
                onClick={() => notify(t("soonGames"))}
              >
                {t("addGame")}
              </Button>
            )}

            {page === "bottles" && (
              <Button
                variant="filled"
                icon={<Icon.Plus size={15} />}
                onClick={() => notify(t("soonBottles"))}
              >
                {t("newBottle")}
              </Button>
            )}

            {page === "logs" && (
              <>
                <Button
                  variant="plain"
                  icon={<Icon.Trash size={15} />}
                  onClick={() => notify(t("soonLogs"))}
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
              initial={{ opacity: 0, y: reduce ? 0 : 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: reduce ? 0 : -8 }}
              transition={pageSpring}
            >
              {page === "home" && (
                <Home
                  go={setPage}
                  notify={notify}
                  sys={sys}
                  scanning={scanning}
                  onScan={() => void scan()}
                />
              )}
              {page === "library" && <Library notify={notify} />}
              {page === "bottles" && <Bottles notify={notify} />}
              {page === "settings" && <Settings notify={notify} sys={sys} />}
              {page === "logs" && <Logs logs={logs} onClear={() => void clear()} />}
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
              transition={spring}
            >
              {toast.msg}
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}

// ============================================================
//  عناصر واجهة صغيرة (بأسلوب آبل)
// ============================================================

function Button({
  children,
  variant = "plain",
  icon,
  onClick,
  disabled,
}: {
  children: React.ReactNode;
  variant?: "filled" | "tinted" | "plain";
  icon?: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
}) {
  const reduce = useReducedMotion();
  return (
    <motion.button
      className={"btn " + variant}
      onClick={onClick}
      disabled={disabled}
      // رد فعل فوري عند الضغط (feedback on pointer-down)
      whileTap={disabled || reduce ? undefined : { scale: 0.97 }}
      transition={{ type: "spring", stiffness: 700, damping: 30, mass: 0.6 }}
    >
      {icon && <span className="btn-icon">{icon}</span>}
      <span>{children}</span>
    </motion.button>
  );
}

function Card({
  title,
  icon,
  children,
  className = "",
}: {
  title?: string;
  icon?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  const reduce = useReducedMotion();
  return (
    <motion.section
      className={"card " + className}
      // رفع خفيف عند المرور (بأسلوب آبل: بلا مبالغة)
      whileHover={reduce ? undefined : { y: -2 }}
      transition={{ type: "spring", stiffness: 420, damping: 34 }}
    >
      {title && (
        <header className="card-head">
          {icon && <span className="card-ico">{icon}</span>}
          <h2>{title}</h2>
        </header>
      )}
      <div className="card-body">{children}</div>
    </motion.section>
  );
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="row">
      <div className="row-text">
        <span className="row-label">{label}</span>
        {hint && <span className="row-hint">{hint}</span>}
      </div>
      {children && <div className="row-control">{children}</div>}
    </div>
  );
}

function Segmented<T extends string>({
  id,
  value,
  options,
  onChange,
}: {
  id: string;
  value: T;
  options: { value: T; label: string; icon?: IconComp }[];
  onChange: (v: T) => void;
}) {
  const reduce = useReducedMotion();
  const spring = reduce
    ? { duration: 0 }
    : { type: "spring" as const, stiffness: 560, damping: 42 };
  return (
    <div className="segmented" role="tablist">
      {options.map((o) => {
        const active = o.value === value;
        const IconC = o.icon;
        return (
          <button
            key={o.value}
            role="tab"
            aria-selected={active}
            className={"seg" + (active ? " active" : "")}
            onClick={() => onChange(o.value)}
          >
            {active && (
              <motion.span
                layoutId={"seg-" + id}
                className="seg-pill"
                transition={spring}
              />
            )}
            {IconC && (
              <span className="seg-icon">
                <IconC size={13.5} />
              </span>
            )}
            <span className="seg-label">{o.label}</span>
          </button>
        );
      })}
    </div>
  );
}

function StatusRow({
  label,
  state,
  value,
}: {
  label: string;
  state: "ok" | "off" | "busy" | "na";
  value?: string;
}) {
  const { t } = useI18n();
  const text =
    value ??
    (state === "ok"
      ? t("ready")
      : state === "busy"
        ? t("scanningLabel")
        : state === "na"
          ? t("unavailable")
          : t("notInstalled"));
  const dotClass = state === "ok" ? "ok" : state === "na" ? "na" : "off";

  return (
    <div className="status-row">
      <span className="status-label">{label}</span>
      <span className="status-value">
        <span className={"dot " + dotClass} />
        {text}
      </span>
    </div>
  );
}

/** سطر معلومة في كرت الجهاز */
function DeviceRow({ k, v }: { k: string; v: string }) {
  return (
    <div className="device-row">
      <span className="device-key">{k}</span>
      <span className="device-val" title={v}>
        {v}
      </span>
    </div>
  );
}

function Empty({
  icon,
  title,
  desc,
  action,
}: {
  icon: React.ReactNode;
  title: string;
  desc: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="empty">
      <span className="empty-ico">{icon}</span>
      <h2>{title}</h2>
      <p className="muted">{desc}</p>
      {action}
    </div>
  );
}

// ============================================================
//  الصفحات
// ============================================================

function Home({
  go,
  notify,
  sys,
  scanning,
  onScan,
}: {
  go: (p: Page) => void;
  notify: (m: string) => void;
  sys: SystemInfo | null;
  scanning: boolean;
  onScan: () => void;
}) {
  const { t } = useI18n();
  const reduce = useReducedMotion();
  const spring = reduce
    ? { duration: 0 }
    : { type: "spring" as const, stiffness: 460, damping: 36 };

  return (
    <div className="stack">
      {/* ترحيب */}
      <motion.section
        className="hero"
        initial={{ opacity: 0, y: reduce ? 0 : 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={spring}
      >
        <span className="hero-ico">
          <Icon.Sparkle size={20} />
        </span>
        <h1 className="hero-title">{t("greeting")}</h1>
        <p className="hero-sub muted">{t("greetingSub")}</p>
        <div className="hero-actions">
          <Button
            variant="filled"
            icon={<Icon.Download size={15} />}
            onClick={() => notify(t("soonEngine"))}
          >
            {t("installEngine")}
          </Button>
          <Button
            variant="tinted"
            icon={<Icon.Plus size={15} />}
            onClick={() => go("library")}
          >
            {t("addGame")}
          </Button>
        </div>
      </motion.section>

      {/* كروت الحالة */}
      <div className="grid">
        <Card title={t("readiness")} icon={<Icon.Chip size={16} />}>
          <StatusRow
            label={t("rosetta")}
            state={
              !sys
                ? "busy"
                : sys.is_apple_silicon
                  ? sys.rosetta_installed
                    ? "ok"
                    : "off"
                  : "na"
            }
          />
          <StatusRow
            label={t("wineEngine")}
            state={!sys ? "busy" : sys.engine_installed ? "ok" : "off"}
          />
          <StatusRow
            label={t("graphicsDxmt")}
            state={!sys ? "busy" : sys.graphics_layer ? "ok" : "off"}
            value={sys?.graphics_layer ?? undefined}
          />
        </Card>

        <Card title={t("yourLibrary")} icon={<Icon.Gamepad size={16} />}>
          <div className="stat-figure">0</div>
          <p className="muted small">{t("noGames")}</p>
          <Button variant="plain" onClick={() => go("library")}>
            {t("openLibrary")}
          </Button>
        </Card>

        <Card title={t("yourDevice")} icon={<Icon.Monitor size={16} />}>
          {scanning ? (
            <p className="muted small">{t("scanningLabel")}</p>
          ) : !sys ? (
            <p className="muted small">{t("unavailable")}</p>
          ) : (
            <div className="device-list">
              <DeviceRow
                k={t("systemLabel")}
                v={`${sys.os_name} ${sys.os_version}`.trim()}
              />
              <DeviceRow
                k={t("chipLabel")}
                v={
                  `${sys.chip || sys.arch}${
                    sys.cpu_cores ? ` · ${sys.cpu_cores} ${t("coresSuffix")}` : ""
                  }`
                }
              />
              <DeviceRow
                k={t("memoryLabel")}
                v={
                  sys.memory_mb
                    ? `${Math.round(sys.memory_mb / 1024)} GB`
                    : t("unavailable")
                }
              />
              <DeviceRow
                k={t("graphicsLabel")}
                v={
                  sys.gpus.length > 0
                    ? `${sys.gpus[0].name}${
                        sys.gpus[0].cores
                          ? ` · ${sys.gpus[0].cores} ${t("coresSuffix")}`
                          : ""
                      }`
                    : t("unavailable")
                }
              />
            </div>
          )}
          <Button variant="plain" onClick={onScan}>
            {scanning ? t("scanningLabel") : t("refresh")}
          </Button>
        </Card>
      </div>

      {/* الخطوات */}
      <Card title={t("stepsTitle")} icon={<Icon.Bolt size={16} />}>
        <ol className="steps">
          {(
            [
              ["step1Title", "step1Desc"],
              ["step2Title", "step2Desc"],
              ["step3Title", "step3Desc"],
            ] as [TKey, TKey][]
          ).map(([titleKey, descKey], i) => (
            <li key={titleKey}>
              <span className="step-num">{i + 1}</span>
              <div className="step-text">
                <b>{t(titleKey)}</b>
                <p className="muted small">{t(descKey)}</p>
              </div>
            </li>
          ))}
        </ol>
      </Card>
    </div>
  );
}

function Library({ notify }: { notify: (m: string) => void }) {
  const { t } = useI18n();
  const [filter, setFilter] = useState<"all" | "steam" | "files">("all");

  return (
    <div className="stack">
      <div className="filter-row">
        <Segmented
          id="library-filter"
          value={filter}
          onChange={setFilter}
          options={[
            { value: "all", label: t("filterAll") },
            { value: "steam", label: t("filterSteam") },
            { value: "files", label: t("filterFiles") },
          ]}
        />
      </div>

      <Card>
        <Empty
          icon={<Icon.Gamepad size={26} />}
          title={t("libraryEmptyTitle")}
          desc={t("libraryEmptyDesc")}
          action={
            <Button
              variant="filled"
              icon={<Icon.Plus size={15} />}
              onClick={() => notify(t("soonGames"))}
            >
              {t("addGame")}
            </Button>
          }
        />
      </Card>
    </div>
  );
}

function Bottles({ notify }: { notify: (m: string) => void }) {
  const { t } = useI18n();
  return (
    <div className="stack">
      <Card>
        <Empty
          icon={<Icon.Bottle size={26} />}
          title={t("bottlesEmptyTitle")}
          desc={t("bottlesEmptyDesc")}
          action={
            <Button
              variant="filled"
              icon={<Icon.Plus size={15} />}
              onClick={() => notify(t("soonBottles"))}
            >
              {t("newBottle")}
            </Button>
          }
        />
      </Card>

      <Card title={t("bottleWhatTitle")} icon={<Icon.Sparkle size={16} />}>
        <p className="muted">{t("bottleWhatDesc")}</p>
      </Card>
    </div>
  );
}

function Settings({
  notify,
  sys,
}: {
  notify: (m: string) => void;
  sys: SystemInfo | null;
}) {
  const { t, choice: langChoice, setChoice: setLangChoice } = useI18n();
  const { theme, setTheme } = useTheme();
  const [gfx, setGfx] = useState<"dxmt" | "dxvk" | "wined3d">("dxmt");

  return (
    <div className="stack narrow">
      {/* المظهر */}
      <Card title={t("appearance")} icon={<Icon.Sun size={16} />}>
        <Row label={t("appearance")}>
          <Segmented
            id="theme"
            value={theme}
            onChange={(v) => {
              setTheme(v);
              notify(t("saved"));
            }}
            options={[
              { value: "light", label: t("themeLight"), icon: Icon.Sun },
              { value: "dark", label: t("themeDark"), icon: Icon.Moon },
              { value: "system", label: t("themeSystem"), icon: Icon.Monitor },
            ]}
          />
        </Row>
        <Row label={t("font")} hint={t("fontDesc")}>
          <span className="value muted">{t("fontSystem")}</span>
        </Row>
      </Card>

      {/* اللغة */}
      <Card title={t("language")} icon={<Icon.Globe size={16} />}>
        <Row label={t("language")} hint={t("languageSystem")}>
          <Segmented
            id="lang"
            value={langChoice}
            onChange={(v) => {
              setLangChoice(v);
            }}
            options={[
              { value: "ar", label: "العربية" },
              { value: "en", label: "English" },
              { value: "system", label: t("themeSystem") },
            ]}
          />
        </Row>
      </Card>

      {/* المحرك والرسومات */}
      <Card title={t("engineSection")} icon={<Icon.Chip size={16} />}>
        <Row
          label={t("wineEngine")}
          hint={sys?.engine_installed ? t("ready") : "Wine 11.x"}
        >
          <Button variant="tinted" onClick={() => notify(t("soonEngine"))}>
            {t("install")}
          </Button>
        </Row>
        <Row label={t("graphicsLayer")}>
          <Segmented
            id="gfx"
            value={gfx}
            onChange={(v) => {
              setGfx(v);
              notify(t("saved"));
            }}
            options={[
              { value: "dxmt", label: "DXMT" },
              { value: "dxvk", label: "DXVK" },
              { value: "wined3d", label: "WineD3D" },
            ]}
          />
        </Row>
      </Card>
    </div>
  );
}

function Logs({
  logs,
  onClear,
}: {
  logs: LogEntry[];
  onClear: () => void;
}) {
  const { t } = useI18n();
  const endRef = useRef<HTMLDivElement>(null);

  // نتابع آخر سطر تلقائياً (مثل تطبيقات الطرفية)
  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [logs.length]);

  return (
    <div className="stack">
      <Card title={t("logsTitle")} icon={<Icon.Doc size={16} />}>
        <div className="loglist">
          {logs.length === 0 ? (
            <span className="muted">{t("logsEmpty")}</span>
          ) : (
            logs.map((l, i) => (
              <div className="logline" key={`${l.time_ms}-${i}`}>
                <span className="log-time">
                  {new Date(l.time_ms).toLocaleTimeString()}
                </span>
                <span className={"log-level " + l.level}>
                  {l.level.toUpperCase()}
                </span>
                <span className="log-msg">{l.message}</span>
              </div>
            ))
          )}
          <div ref={endRef} />
        </div>

        <div className="log-actions">
          <Button
            variant="plain"
            icon={<Icon.Trash size={15} />}
            onClick={onClear}
          >
            {t("clearLogs")}
          </Button>
        </div>
      </Card>
    </div>
  );
}
