// ============================================================
//  جسر — الصفحات الخمس
//  الرئيسية · المكتبة · القوارير · الإعدادات · السجل
//  كل صفحة memo ⇒ لا تُعاد رسمها إلا لما تخصها يتغيّر فقط
// ============================================================
import { memo, useCallback, useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useI18n, type TKey } from "./i18n";
import { useTheme } from "./useTheme";
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
import {
  debugLogPath,
  exportDebugReport,
  getGraphicsMode,
  setGraphicsMode,
  steamGameLaunch,
  steamGameWindowed,
  steamGamesList,
  type Bottle,
  type DownloadProgress,
  type Game,
  type GraphicsMode,
  type LogEntry,
  type SteamGame,
  type SystemInfo,
} from "./api";
import {
  ENGINE_ID,
  engineButtonState,
  fmtBytes,
  fmtSpeed,
  isActive,
  type Downloads,
} from "./useDownloads";
import type { Bottles } from "./useBottles";
import type { Updates } from "./useUpdates";
import { fmtPlayed, type Games } from "./useGames";
import { STEAM_SETUP_ID, type Steam } from "./useSteam";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

export type Page = "home" | "library" | "bottles" | "settings" | "logs";

// ============================================================
//  صندوق التنزيل (يُعرض في الرئيسية والإعدادات)
// ============================================================
function EngineDownloadBox({
  p,
  installed,
  onCancel,
}: {
  p: DownloadProgress;
  installed?: boolean;
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
  } else if (p.phase === "extracting") {
    const pct = p.total > 0 ? Math.round((p.received / p.total) * 100) : -1;
    const filesStr = `${p.files.toLocaleString("en-US")} ${t("filesSuffix")}`;
    label = `${t("dlExtracting")}${pct >= 0 ? ` ${pct}%` : ""} · ${filesStr}`;
  } else if (p.phase === "done") {
    label = installed ? t("dlReady") : t("dlDone");
  } else if (p.phase === "cancelled") {
    label = t("dlCancelled");
  } else {
    const failLabel = p.op === "install" ? t("dlInstallFailed") : t("dlFailed");
    label = failLabel + (p.error ? ` — ${p.error}` : "");
  }

  return (
    <div className="dl">
      <div className="dl-head">
        <span
          className={
            "dot " +
            (p.phase === "done"
              ? "ok"
              : p.phase === "error"
                ? "danger"
                : active
                  ? "busy"
                  : "off")
          }
        />
        <span
          className={
            "dl-label" +
            (p.phase === "done" ? " ok" : p.phase === "error" ? " danger" : "")
          }
          title={label}
        >
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
      {p.phase === "done" && <ProgressBar value={1} tone="ok" />}
      {p.phase === "error" && <ProgressBar value={1} tone="danger" />}
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
  games,
}: {
  go: (p: Page) => void;
  sys: SystemInfo | null;
  scanning: boolean;
  onScan: () => void;
  dl: Downloads;
  games: Games;
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
                : engineBtn === "ready"
                  ? t("ready")
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
            installed={!!sys?.engine_installed}
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
          <StatusRow
            label={t("gstreamerLabel")}
            state={
              !sys
                ? "busy"
                : sys.os_name !== "macOS"
                  ? "na"
                  : sys.gstreamer_installed
                    ? "ok"
                    : "off"
            }
            value={sys?.gstreamer_version ?? undefined}
          />
        </Card>

        <Card title={t("yourLibrary")} icon={<Icon.Gamepad size={16} />}>
          <div className="stat-figure">{games.items.length}</div>
          <p className="muted small">
            {games.items.length > 0 ? t("libraryGamesIn") : t("noGames")}
          </p>
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
  games,
  bottles,
  steam,
  dl,
  notify,
}: {
  games: Games;
  bottles: Bottles;
  steam: Steam;
  dl: Downloads;
  notify: (msg: string) => void;
}) {
  const { t } = useI18n();
  const [filter, setFilter] = useState<"all" | "steam" | "files">("all");
  const [steamGames, setSteamGames] = useState<SteamGame[]>([]);
  const [steamLoading, setSteamLoading] = useState(false);
  const steamInstalled = !!steam.status?.installed;
  const steamRunning = !!steam.status?.running;

  const loadSteamGames = useCallback(() => {
    if (!steamInstalled) {
      setSteamGames([]);
      return;
    }
    setSteamLoading(true);
    steamGamesList()
      .then(setSteamGames)
      .catch(() => setSteamGames([]))
      .finally(() => setSteamLoading(false));
  }, [steamInstalled]);

  // نقرأ مكتبة ستيم عند التثبيت، وعند كل عودة للصفحة (Steam قد ينزّل لعبة)
  useEffect(() => {
    loadSteamGames();
  }, [loadSteamGames]);

  const launchSteamGame = useCallback(
    (g: SteamGame) => {
      steamGameLaunch(g.appid)
        .then(() => notify(t("steamGameLaunched", { name: g.name })))
        .catch((err) => notify(String(err)));
    },
    [notify, t],
  );

  // «تشغيل في نافذة»: يمنع اللعبة من أخذ شاشة الماك بالكامل (شاشة سودا)
  const windowedSteamGame = useCallback(
    (g: SteamGame) => {
      steamGameWindowed(g.appid, true)
        .then(() => notify(t("windowedApplied", { name: g.name })))
        .catch((err) => notify(String(err)));
    },
    [notify, t],
  );

  const mainReady = bottles.items.some(
    (b) => b.kind === "main" && b.state === "ready",
  );
  // فلتر «ستيم» يخص مكتبة ستيم، و«ملفات» يخص الألعاب المضافة يدوياً
  const showSteam = steamInstalled && filter !== "files";
  const showManual = filter !== "steam";
  const filtered = games.items.filter((g) => g.kind !== "steam");

  return (
    <div className="stack">
      <SteamCard steam={steam} dl={dl} />

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

      {showSteam && (
        <SteamLibrary
          games={steamGames}
          loading={steamLoading}
          running={steamRunning}
          onRefresh={loadSteamGames}
          onPlay={launchSteamGame}
          onWindowed={windowedSteamGame}
        />
      )}

      {showManual &&
        (games.items.length === 0 ? (
          // إذا فيه ألعاب ستيم معروضة، ما نغطيها بكرت «فاضية» — زر الإضافة فوق
          (filter === "files" || steamGames.length === 0) && (
            <Card>
              <Empty
                icon={<Icon.Gamepad size={26} />}
                title={t("libraryEmptyTitle")}
                desc={t("libraryEmptyDesc")}
                action={
                  <Button
                    variant="filled"
                    icon={<Icon.Plus size={15} />}
                    onClick={() => games.add()}
                  >
                    {t("addGame")}
                  </Button>
                }
              />
            </Card>
          )
        ) : (
          <>
            {!mainReady && (
              <p className="muted small">{t("gameNeedBottle")}</p>
            )}
            {filtered.map((g) => (
              <GameCard key={g.id} g={g} games={games} bottleReady={mainReady} />
            ))}
            {filtered.length === 0 && (
              <Card>
                <p className="muted">{t("libraryNoMatch")}</p>
              </Card>
            )}
          </>
        ))}

      <Card title={t("libraryNoteTitle")} icon={<Icon.Sparkle size={16} />}>
        <p className="muted">{t("libraryNoteDesc")}</p>
      </Card>
    </div>
  );
});

// ============================================================
//  مكتبة ستيم — ألعاب مثبّتة داخل القارورة (تُقرأ من appmanifest)
// ============================================================
const SteamLibrary = memo(function SteamLibrary({
  games,
  loading,
  running,
  onRefresh,
  onPlay,
  onWindowed,
}: {
  games: SteamGame[];
  loading: boolean;
  running: boolean;
  onRefresh: () => void;
  onPlay: (g: SteamGame) => void;
  onWindowed: (g: SteamGame) => void;
}) {
  const { t } = useI18n();
  return (
    <Card title={t("steamLibraryTitle")} icon={<Icon.Gamepad size={16} />}>
      <div className="bottle-row">
        <span
          className={
            "dot " + (loading ? "busy" : games.length > 0 ? "ok" : "off")
          }
        />
        <span className="value">
          {loading
            ? t("steamLibraryLoading")
            : t("steamLibraryCount", { n: games.length })}
        </span>
        <Button variant="plain" onClick={onRefresh} disabled={loading}>
          {t("refresh")}
        </Button>
      </div>

      {!loading && games.length === 0 && (
        <p className="muted small">{t("steamLibraryEmpty")}</p>
      )}

      {games.length > 0 && (
        <div className="steam-games">
          {games.map((g) => (
            <div className="steam-game" key={g.appid}>
              <div className="steam-game-info">
                <span className="steam-game-name">{g.name}</span>
                <span className="muted small">
                  {t("steamAppId", { id: g.appid })}
                  {g.size_bytes > 0 ? ` · ${fmtBytes(g.size_bytes)}` : ""}
                </span>
              </div>
              <Button
                variant="filled"
                icon={<Icon.Play size={13} />}
                disabled={!running}
                onClick={() => onPlay(g)}
              >
                {t("gamePlay")}
              </Button>
              {/* تشغيل في نافذة: يمنع الشاشة السوداء مع الألعاب القديمة */}
              <Button
                variant="plain"
                title={t("windowedHint")}
                onClick={() => onWindowed(g)}
              >
                {t("windowedBtn")}
              </Button>
            </div>
          ))}
        </div>
      )}

      <p className="muted small">
        {running ? t("steamLibraryHint") : t("steamLibraryNeedRun")}
      </p>
    </Card>
  );
});

// ============================================================
//  بطاقة ستيم — المسار الرسمي (تنزيل المثبّت → تثبيت داخل القارورة → تشغيل)
// ============================================================
const SteamCard = memo(function SteamCard({
  steam,
  dl,
}: {
  steam: Steam;
  dl: Downloads;
}) {
  const { t } = useI18n();
  const s = steam.status;
  const p = dl.items[STEAM_SETUP_ID];
  const du = isActive(p);
  const running = !!s?.running;
  // الألوان تشرح الحالة: أخضر=جاهز/شغّال · أزرق=جاري عمل · كهرماني=يحتاج ضغطة منك · رمادي=ما بدأنا
  const dotCls = !s
    ? "off"
    : running
      ? "live"
      : s.installing || du
        ? "busy"
        : s.installed
          ? "ok"
          : s.installer_ready
            ? "warn"
            : "off";
  const valCls = !s
    ? ""
    : running || s.installed
      ? " ok"
      : s.installing || du
        ? " busy"
        : s.installer_ready
          ? " warn"
          : "";

  return (
    <Card title={t("steamCardTitle")} icon={<Icon.Download size={16} />}>
      <div className="bottle-row">
        <span className={"dot " + dotCls} />
        <span className={"value" + valCls}>
          {!s
            ? t("dlPreparing")
            : running
              ? t("steamRunningNow")
              : s.installing
                ? t("steamInstalling")
                : s.installed
                  ? t("steamStepRun")
                  : s.installer_ready
                    ? t("steamInstallerReady")
                    : t("steamStepDownload")}
        </span>
      </div>

      {!s ? null : !s.bottle_ready ? (
        <p className="muted small">{t("steamNeedBottle")}</p>
      ) : s.installed ? (
        <div className="row-actions">
          {running ? (
            <Button
              variant="danger"
              icon={<Icon.Stop size={13} />}
              onClick={steam.stop}
            >
              {t("steamStop")}
            </Button>
          ) : (
            <Button
              variant="filled"
              icon={<Icon.Play size={13} />}
              onClick={steam.run}
            >
              {t("steamRunBtn")}
            </Button>
          )}
        </div>
      ) : s.installing ? (
        <div className="row-actions">
          <Button variant="danger" onClick={steam.stop}>
            {t("steamStop")}
          </Button>
        </div>
      ) : s.installer_ready ? (
        <div className="row-actions">
          <Button
            variant="filled"
            icon={<Icon.Download size={15} />}
            onClick={steam.install}
          >
            {t("steamInstallBtn")}
          </Button>
        </div>
      ) : (
        <>
          {p && (isActive(p) || p.phase === "error" || p.phase === "cancelled") && (
            <EngineDownloadBox p={p} onCancel={() => dl.cancel(STEAM_SETUP_ID)} />
          )}
          {!isActive(p) && (
            <div className="row-actions">
              <Button
                variant="filled"
                icon={<Icon.Download size={15} />}
                onClick={() => dl.start(STEAM_SETUP_ID)}
              >
                {t("steamDownloadBtn")}
              </Button>
            </div>
          )}
        </>
      )}

      <p className="muted small">
        {s?.installed
          ? t("steamNote")
          : s?.installer_ready
            ? t("steamStepInstallHint")
            : isActive(p)
              ? t("steamStepDownloadHint")
              : t("steamDesc")}
      </p>
    </Card>
  );
});

/** بطاقة لعبة: الحالة + وقت اللعب + الأزرار حسب الحالة */
const GameCard = memo(function GameCard({
  g,
  games,
  bottleReady,
}: {
  g: Game;
  games: Games;
  bottleReady: boolean;
}) {
  const { t } = useI18n();
  const [confirm, setConfirm] = useState(false);
  const running = g.running;

  return (
    <Card title={g.name} icon={<Icon.Gamepad size={16} />}>
      <div className="bottle-row">
        <span className={"dot " + (running ? "live" : "ok")} />
        <span className={"value" + (running ? " ok" : "")}>
          {running
            ? t("gameRunningNow")
            : t("gamePlayed", { dur: fmtPlayed(g.played_ms) })}
        </span>
        <span className="muted small">
          {g.kind === "steam" ? t("filterSteam") : t("filterFiles")}
        </span>
      </div>

      <div className="row-actions">
        {running ? (
          <Button
            variant="danger"
            icon={<Icon.Stop size={13} />}
            onClick={() => games.stop(g.id)}
          >
            {t("gameStop")}
          </Button>
        ) : (
          <Button
            variant="filled"
            icon={<Icon.Play size={13} />}
            disabled={!bottleReady}
            onClick={() => games.launch(g.id)}
          >
            {t("gamePlay")}
          </Button>
        )}

        <Button
          variant="plain"
          onClick={() => void revealItemInDir(g.exe).catch(() => {})}
        >
          {t("gameReveal")}
        </Button>

        {!running && !confirm && (
          <Button
            variant="plain"
            icon={<Icon.Trash size={15} />}
            onClick={() => setConfirm(true)}
          >
            {t("gameRemove")}
          </Button>
        )}
      </div>

      {confirm && (
        <div className="row-actions">
          <span className="muted small">{t("gameRemoveWarn")}</span>
          <Button
            variant="danger"
            onClick={() => {
              games.remove(g.id);
              setConfirm(false);
            }}
          >
            {t("gameRemoveYes")}
          </Button>
          <Button variant="plain" onClick={() => setConfirm(false)}>
            {t("gameRemoveNo")}
          </Button>
        </div>
      )}
    </Card>
  );
});

/** اسم القارورة من نوعها — يُترجم حسب لغة الواجهة (الاسم مو نص مخزّن) */
function bottleName(kind: string, t: (k: TKey) => string): string {
  return t(kind === "main" ? "bottleMainName" : "bottlePrivateName");
}

/** سطور سجل القوارير تجي بنوعها (main/private) — نحوّله لاسم مترجم قبل العرض */
function logParams(
  params: Record<string, unknown> | undefined,
  t: (k: TKey) => string,
): Record<string, unknown> | undefined {
  const kind = params?.kind;
  if (kind !== "main" && kind !== "private") return params;
  return { ...params, name: bottleName(kind, t) };
}

// ============================================================
//  القوارير
// ============================================================
export const BottlesPage = memo(function BottlesPage({
  bottles,
  sys,
}: {
  bottles: Bottles;
  sys: SystemInfo | null;
}) {
  const { t } = useI18n();
  const main = bottles.items.find((b) => b.kind === "main");
  const privates = bottles.items.filter((b) => b.kind !== "main");
  const engineReady = !!sys?.engine_installed;

  return (
    <div className="stack">
      {!main ? (
        <Card>
          <Empty
            icon={<Icon.Bottle size={26} />}
            title={t("bottleSetupTitle")}
            desc={engineReady ? t("bottleSetupDesc") : t("bottleNeedEngine")}
            action={
              <Button
                variant="filled"
                icon={<Icon.Bolt size={15} />}
                disabled={!engineReady}
                onClick={() => bottles.create("main")}
              >
                {t("bottleSetup")}
              </Button>
            }
          />
        </Card>
      ) : (
        <BottleCard b={main} bottles={bottles} sys={sys} />
      )}

      {privates.map((b) => (
        <BottleCard key={b.id} b={b} bottles={bottles} sys={sys} />
      ))}

      <div className="row-actions">
        <Button
          variant="plain"
          icon={<Icon.Plus size={15} />}
          disabled={!engineReady}
          onClick={() => bottles.create("private")}
        >
          {t("newBottle")}
        </Button>
      </div>

      <Card title={t("bottleWhatTitle")} icon={<Icon.Sparkle size={16} />}>
        <p className="muted">{t("bottleWhatDesc")}</p>
      </Card>
    </div>
  );
});

/** بطاقة قارورة واحدة: النبض (الحالة) + الحجم + الأزرار حسب الحالة */
const BottleCard = memo(function BottleCard({
  b,
  bottles,
  sys,
}: {
  b: Bottle;
  bottles: Bottles;
  sys: SystemInfo | null;
}) {
  const { t } = useI18n();
  const [confirm, setConfirm] = useState(false);
  const busy = b.state === "creating";
  const ready = b.state === "ready";
  const isWindows = (sys?.os_name ?? "").toLowerCase().includes("windows");

  return (
    <Card
      title={bottleName(b.kind, t)}
      icon={<Icon.Bottle size={16} />}
    >
      <div className="bottle-row">
        <span className={"dot " + (ready ? "ok" : busy ? "busy" : "danger")} />
        <span className={"value " + (ready ? "ok" : busy ? "busy" : "danger")}>
          {busy
            ? t("bottleSettingUp")
            : ready
              ? t("bottleReadyNow")
              : t("bottleFailedTitle")}
        </span>
        <span className="muted small">
          {b.size_bytes > 0 ? fmtBytes(b.size_bytes) : ""}
        </span>
      </div>

      {busy && <ProgressBar value={-1} />}
      {busy && <p className="muted small">{t("bottleSettingUpHint")}</p>}
      {!busy && b.state === "error" && b.error && (
        <p className="muted small">{b.error}</p>
      )}

      <div className="row-actions">
        {b.state === "error" && (
          <Button variant="tinted" onClick={() => bottles.retry(b.id)}>
            {t("bottleRetry")}
          </Button>
        )}
        {b.state === "error" && isWindows && (
          <Button variant="plain" onClick={() => bottles.retry(b.id, true)}>
            {t("bottleDevSim")}
          </Button>
        )}
        {b.kind !== "main" && !busy && !confirm && (
          <Button
            variant="plain"
            icon={<Icon.Trash size={15} />}
            onClick={() => setConfirm(true)}
          >
            {t("bottleDelete")}
          </Button>
        )}
      </div>

      {confirm && (
        <div className="row-actions">
          <span className="muted small">{t("bottleDeleteWarn")}</span>
          <Button
            variant="danger"
            onClick={() => {
              bottles.remove(b.id);
              setConfirm(false);
            }}
          >
            {t("bottleDeleteYes")}
          </Button>
          <Button variant="plain" onClick={() => setConfirm(false)}>
            {t("bottleDeleteNo")}
          </Button>
        </div>
      )}
    </Card>
  );
});

// ============================================================
//  الإعدادات — كل تغيير يُحفظ تلقائياً وفوراً
// ============================================================
export const SettingsPage = memo(function SettingsPage({
  sys,
  dl,
  updates,
}: {
  sys: SystemInfo | null;
  dl: Downloads;
  updates: Updates;
}) {
  const { t, choice: langChoice, setChoice: setLangChoice } = useI18n();
  const { theme, setTheme } = useTheme();
  // وضع الرسومات الحقيقي (يُحفظ في الخلفية ويُطبَّق على التشغيل التالي)
  const [gfx, setGfxState] = useState<GraphicsMode>("auto");
  useEffect(() => {
    getGraphicsMode()
      .then(setGfxState)
      .catch(() => {});
  }, []);
  const setGfx = useCallback((mode: GraphicsMode) => {
    setGfxState(mode);
    setGraphicsMode(mode).catch(() => {});
  }, []);

  // app version shown in the updates card
  const [appVer, setAppVer] = useState<string | null>(null);
  useEffect(() => {
    getVersion()
      .then(setAppVer)
      .catch(() => {});
  }, []);

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
              {engineBtn === "busy"
                ? t("installing")
                : engineBtn === "done"
                  ? t("dlDone")
                  : t("ready")}
            </span>
          )}
        </Row>
        {engineDl && (
          <EngineDownloadBox
            p={engineDl}
            installed={!!sys?.engine_installed}
            onCancel={() => dl.cancel(ENGINE_ID)}
          />
        )}
        <Row label={t("graphicsLayer")} hint={t("gfxHint")}>
          <Segmented
            id="gfx"
            value={gfx}
            onChange={setGfx}
            options={[
              { value: "auto", label: t("gfxAuto") },
              { value: "dxvk", label: "DXVK" },
              { value: "wined3d", label: "WineD3D" },
            ]}
          />
        </Row>
      </Card>

      {/* التحديثات */}
      <Card title={t("updateSection")} icon={<Icon.Download size={16} />}>
        <Row
          label={t("updateLabel")}
          hint={
            (appVer ? `v${appVer} · ` : "") +
            (updates.status === "available"
              ? t("updateAvailable").replace("{v}", updates.version ?? "")
              : updates.status === "ready"
                ? t("updateInstalled")
                : updates.status === "upToDate"
                  ? t("updateUpToDate")
                  : updates.status === "error"
                    ? t("updateFailed")
                    : t("updateAuto"))
          }
        >
          {updates.status === "available" ? (
            <Button variant="tinted" onClick={() => updates.install()}>
              {t("updateInstall")}
            </Button>
          ) : updates.status === "downloading" ? (
            <span className="value muted">
              {updates.progress != null
                ? `${updates.progress}%`
                : t("updateDownloading")}
            </span>
          ) : updates.status === "ready" ? (
            <Button variant="tinted" onClick={() => updates.restart()}>
              {t("updateRestart")}
            </Button>
          ) : (
            <Button
              variant="plain"
              disabled={updates.status === "checking"}
              onClick={() => updates.check()}
            >
              {updates.status === "checking" ? t("updateChecking") : t("updateCheck")}
            </Button>
          )}
        </Row>
        {updates.error && (
          <p className="muted small">{updates.error}</p>
        )}
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
  notify,
}: {
  logs: LogEntry[];
  onClear: () => void;
  notify: (msg: string) => void;
}) {
  const { t } = useI18n();
  const endRef = useRef<HTMLDivElement>(null);
  const [busy, setBusy] = useState(false);

  // نتابع آخر سطر تلقائياً (مثل تطبيقات الطرفية)
  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [logs.length]);

  // «انسخ السجل كامل»: تقرير واحد فيه الجهاز + كل السجل + سجلات ستيم والألعاب
  const onCopyFull = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    try {
      const report = await exportDebugReport();
      await navigator.clipboard.writeText(report);
      notify(t("logCopied"));
    } catch {
      notify(t("logCopyFailed"));
    } finally {
      setBusy(false);
    }
  }, [busy, notify, t]);

  // «إظهار في Finder»: نكشف ملف السجل الدائم — أسهل لو النسخ ما نفع
  const onReveal = useCallback(async () => {
    try {
      const path = await debugLogPath();
      if (path) await revealItemInDir(path);
    } catch {
      /* لا شيء — الزر تحسيني */
    }
  }, []);

  return (
    <div className="stack">
      <Card title={t("logsTitle")} icon={<Icon.Doc size={16} />}>
        <span className="live-tag">
          <span className="dot live" />
          {t("liveLabel")}
        </span>

        <div className="loglist">
          {logs.length === 0 ? (
            <span className="muted">{t("logsEmpty")}</span>
          ) : (
            logs.map((l, i) => {
              // سطور التشخيص الخام تُعرض حرفياً كما كُتبت (بدون ترجمة)
              const isRaw = l.key === "log.raw";
              const rawLine = isRaw ? String(l.params?.line ?? "") : "";
              return (
                <div
                  className={"logline" + (isRaw ? " raw" : "")}
                  key={`${l.time_ms}-${i}`}
                >
                  <span className="log-time">
                    {new Date(l.time_ms).toLocaleTimeString()}
                  </span>
                  <span className={"log-level " + l.level}>
                    {l.level.toUpperCase()}
                  </span>
                  <span className="log-msg">
                    {isRaw ? rawLine : t(l.key as TKey, logParams(l.params, t))}
                  </span>
                </div>
              );
            })
          )}
          <div ref={endRef} />
        </div>

        <div className="log-actions">
          <Button
            variant="plain"
            icon={<Icon.Copy size={15} />}
            disabled={busy}
            onClick={() => void onCopyFull()}
          >
            {t("copyFullLog")}
          </Button>
          <Button
            variant="plain"
            icon={<Icon.Folder size={15} />}
            onClick={() => void onReveal()}
          >
            {t("revealLog")}
          </Button>
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
