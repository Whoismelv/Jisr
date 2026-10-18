// ============================================================
//  جسر — الصفحات الخمس
//  الرئيسية · المكتبة · القوارير · الإعدادات · السجل
//  كل صفحة memo ⇒ لا تُعاد رسمها إلا لما تخصها يتغيّر فقط
// ============================================================
import { memo, useCallback, useEffect, useRef, useState } from "react";
import { useI18n, type TKey } from "./i18n";
import { useTheme } from "./useTheme";
import { useMotion } from "./useMotion";
import {
  Button,
  Card,
  DeviceRow,
  Empty,
  ProgressBar,
  Row,
  Segmented,
  StatusRow,
} from "./ui";
import * as Icon from "./Icons";
import type { DownloadProgress, LogEntry, SystemInfo } from "./api";
import {
  ENGINE_ID,
  engineButtonState,
  fmtBytes,
  fmtSpeed,
  isActive,
  type Downloads,
} from "./useDownloads";

export type Page = "home" | "library" | "bottles" | "settings" | "logs";

/** إعداد محفوظ تلقائياً في localStorage (حفظ فوري — بلا زر حفظ) */
function usePersisted<T extends string>(key: string, initial: T) {
  const [value, setValue] = useState<T>(
    () => (localStorage.getItem(key) as T | null) ?? initial,
  );
  const set = useCallback(
    (v: T) => {
      setValue(v);
      localStorage.setItem(key, v);
    },
    [key],
  );
  return [value, set] as const;
}

// ============================================================
//  صندوق التنزيل (يُعرض في الرئيسية والإعدادات)
// ============================================================
function EngineDownloadBox({
  p,
  onCancel,
}: {
  p: DownloadProgress;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  const active = isActive(p);

  let label: string;
  if (p.phase === "starting") {
    label = t("dlPreparing");
  } else if (p.phase === "downloading") {
    const size = `${fmtBytes(p.received)} / ${p.total > 0 ? fmtBytes(p.total) : "…"}`;
    const spd = fmtSpeed(p.speed_bps);
    label = spd ? `${size} · ${spd}` : size;
  } else if (p.phase === "verifying") {
    label = t("dlVerifying");
  } else if (p.phase === "done") {
    label = t("dlDone");
  } else if (p.phase === "cancelled") {
    label = t("dlCancelled");
  } else {
    label = t("dlFailed") + (p.error ? ` — ${p.error}` : "");
  }

  return (
    <div className="dl">
      <div className="dl-head">
        <span
          className={
            "dot " + (p.phase === "done" ? "ok" : active ? "busy" : "off")
          }
        />
        <span className="dl-label" title={label}>
          {label}
        </span>
        {active && (
          <Button variant="plain" onClick={onCancel}>
            {t("dlCancel")}
          </Button>
        )}
      </div>
      {active && (
        <ProgressBar value={p.total > 0 ? p.received / p.total : -1} />
      )}
    </div>
  );
}

// ============================================================
//  الرئيسية
// ============================================================
export const HomePage = memo(function HomePage({
  go,
  sys,
  scanning,
  onScan,
  dl,
}: {
  go: (p: Page) => void;
  sys: SystemInfo | null;
  scanning: boolean;
  onScan: () => void;
  dl: Downloads;
}) {
  const { t } = useI18n();
  const engineDl = dl.items[ENGINE_ID];
  const engineBtn = engineButtonState(engineDl, !!sys?.engine_installed);

  return (
    <div className="stack">
      {/* ترحيب */}
      <section className="hero">
        <span className="hero-ico">
          <Icon.Sparkle size={20} />
        </span>
        <h1 className="hero-title">{t("greeting")}</h1>
        <p className="hero-sub muted">{t("greetingSub")}</p>
        <div className="hero-actions">
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
                : t("installEngine")}
          </Button>
          <Button
            variant="tinted"
            icon={<Icon.Plus size={15} />}
            onClick={() => go("library")}
          >
            {t("addGame")}
          </Button>
        </div>
      </section>

      {/* شريط التنزيل — يظهر بس لما يكون فيه تنزيل */}
      {engineDl && (
        <div className="dl-panel">
          <EngineDownloadBox
            p={engineDl}
            onCancel={() => dl.cancel(ENGINE_ID)}
          />
        </div>
      )}

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
                v={`${sys.chip || sys.arch}${
                  sys.cpu_cores
                    ? ` · ${sys.cpu_cores} ${t("coresSuffix")}`
                    : ""
                }`}
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
});

// ============================================================
//  المكتبة
// ============================================================
export const LibraryPage = memo(function LibraryPage({
  notify,
}: {
  notify: (m: string) => void;
}) {
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
});

// ============================================================
//  القوارير
// ============================================================
export const BottlesPage = memo(function BottlesPage({
  notify,
}: {
  notify: (m: string) => void;
}) {
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
});

// ============================================================
//  الإعدادات — كل تغيير يُحفظ تلقائياً وفوراً
// ============================================================
export const SettingsPage = memo(function SettingsPage({
  sys,
  dl,
}: {
  sys: SystemInfo | null;
  dl: Downloads;
}) {
  const { t, choice: langChoice, setChoice: setLangChoice } = useI18n();
  const { theme, setTheme } = useTheme();
  const { motion: motionChoice, setMotion } = useMotion();
  const [gfx, setGfx] = usePersisted<"dxmt" | "dxvk" | "wined3d">(
    "jisr.gfx",
    "dxmt",
  );

  const engineDl = dl.items[ENGINE_ID];
  const engineBtn = engineButtonState(engineDl, !!sys?.engine_installed);

  return (
    <div className="stack narrow">
      {/* المظهر */}
      <Card title={t("appearance")} icon={<Icon.Sun size={16} />}>
        <Row label={t("appearance")}>
          <Segmented
            id="theme"
            value={theme}
            onChange={setTheme}
            options={[
              { value: "light", label: t("themeLight"), icon: Icon.Sun },
              { value: "dark", label: t("themeDark"), icon: Icon.Moon },
              { value: "system", label: t("themeAuto"), icon: Icon.Monitor },
            ]}
          />
        </Row>
        <Row label={t("motionLabel")}>
          <Segmented
            id="motion"
            value={motionChoice}
            onChange={setMotion}
            options={[
              { value: "full", label: t("motionFull") },
              { value: "reduced", label: t("motionLite") },
              { value: "system", label: t("themeAuto") },
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
            onChange={setLangChoice}
            options={[
              { value: "ar", label: "العربية" },
              { value: "en", label: "English" },
              { value: "system", label: t("themeAuto") },
            ]}
          />
        </Row>
      </Card>

      {/* المحرك والرسومات */}
      <Card title={t("engineSection")} icon={<Icon.Chip size={16} />}>
        <Row
          label={t("wineEngine")}
          hint={
            sys?.engine_installed
              ? t("ready")
              : engineBtn === "busy"
                ? t("installing")
                : engineBtn === "done"
                  ? t("dlDone")
                  : "Wine 11.18"
          }
        >
          {engineBtn === "idle" ? (
            <Button variant="tinted" onClick={() => dl.start(ENGINE_ID)}>
              {t("install")}
            </Button>
          ) : (
            <span className="value muted">
              {engineBtn === "busy" ? t("installing") : t("dlDone")}
            </span>
          )}
        </Row>
        {engineDl && (
          <EngineDownloadBox
            p={engineDl}
            onCancel={() => dl.cancel(ENGINE_ID)}
          />
        )}
        <Row label={t("graphicsLayer")}>
          <Segmented
            id="gfx"
            value={gfx}
            onChange={setGfx}
            options={[
              { value: "dxmt", label: "DXMT" },
              { value: "dxvk", label: "DXVK" },
              { value: "wined3d", label: "WineD3D" },
            ]}
          />
        </Row>
      </Card>

      <p className="muted small">{t("autoSaveNote")}</p>
    </div>
  );
});

// ============================================================
//  السجل المباشر
// ============================================================
export const LogsPage = memo(function LogsPage({
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
        <span className="live-tag">
          <span className="dot" />
          {t("liveLabel")}
        </span>

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
                <span className="log-msg">{t(l.key as TKey, l.params)}</span>
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
});
