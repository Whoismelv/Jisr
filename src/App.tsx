import { useEffect, useState, type ComponentType } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useI18n, type TKey } from "./i18n";
import { useTheme } from "./useTheme";
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
              {page === "home" && <Home go={setPage} notify={notify} />}
              {page === "library" && <Library notify={notify} />}
              {page === "bottles" && <Bottles notify={notify} />}
              {page === "settings" && <Settings notify={notify} />}
              {page === "logs" && <Logs notify={notify} />}
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
}: {
  label: string;
  state: "ok" | "off" | "busy";
}) {
  const { t } = useI18n();
  const text =
    state === "ok" ? t("ready") : state === "busy" ? t("installing") : t("notInstalled");
  return (
    <div className="status-row">
      <span className="status-label">{label}</span>
      <span className="status-value">
        <span className={"dot " + (state === "ok" ? "ok" : "off")} />
        {text}
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
}: {
  go: (p: Page) => void;
  notify: (m: string) => void;
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
          <StatusRow label={t("rosetta")} state="off" />
          <StatusRow label={t("wineEngine")} state="off" />
          <StatusRow label={t("graphicsDxmt")} state="off" />
        </Card>

        <Card title={t("yourLibrary")} icon={<Icon.Gamepad size={16} />}>
          <div className="stat-figure">0</div>
          <p className="muted small">{t("noGames")}</p>
          <Button variant="plain" onClick={() => go("library")}>
            {t("openLibrary")}
          </Button>
        </Card>

        <Card title={t("yourDevice")} icon={<Icon.Monitor size={16} />}>
          <p className="muted small">{t("detectedAutomatically")}</p>
          <div className="kv">
            <span className="muted">Mac</span>
            <span className="muted">—</span>
          </div>
          <div className="kv">
            <span className="muted">GPU</span>
            <span className="muted">—</span>
          </div>
          <Button variant="plain" onClick={() => notify(t("soonDevice"))}>
            {t("checkDevice")}
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

function Settings({ notify }: { notify: (m: string) => void }) {
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
        <Row label={t("wineEngine")} hint="Wine 11.x">
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

function Logs({ notify }: { notify: (m: string) => void }) {
  const { t } = useI18n();
  return (
    <div className="stack">
      <Card title={t("logsTitle")} icon={<Icon.Doc size={16} />}>
        <div className="logbox">
          <span className="muted">{t("logsEmpty")}</span>
        </div>
        <div className="log-actions">
          <Button
            variant="plain"
            icon={<Icon.Trash size={15} />}
            onClick={() => notify(t("soonLogs"))}
          >
            {t("clearLogs")}
          </Button>
        </div>
      </Card>
    </div>
  );
}
