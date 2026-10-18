// ============================================================
//  جسر — عناصر الواجهة الصغيرة (بأسلوب آبل)
//  Button · Card · Row · Segmented · StatusRow · DeviceRow · Empty
//  كل العناصر memo ⇒ إعادة رسم أقل = أداء أسلس
// ============================================================
import { memo, type ComponentType, type ReactNode } from "react";
import { motion } from "motion/react";
import { useI18n } from "./i18n";
import type { IconProps } from "./Icons";

export type IconComp = ComponentType<IconProps>;

/** نوابض موحّدة: إيقاع قصير بلا ارتداد زايد (فلسفة آبل: استجابة فورية) */
export const SPRINGS = {
  fast: { type: "spring", stiffness: 700, damping: 42, mass: 0.85 },
  page: { type: "spring", stiffness: 480, damping: 40, mass: 0.9 },
  toast: { type: "spring", stiffness: 560, damping: 38, mass: 0.8 },
} as const;

// ---------- زر ----------
export const Button = memo(function Button({
  children,
  variant = "plain",
  icon,
  onClick,
  disabled,
  title,
}: {
  children: ReactNode;
  variant?: "filled" | "tinted" | "plain";
  icon?: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <motion.button
      className={"btn " + variant}
      onClick={onClick}
      disabled={disabled}
      title={title}
      // ردّ فعل فوري عند الضغط (يُلغى تلقائياً مع تقليل الحركة)
      whileTap={disabled ? undefined : { scale: 0.97 }}
      transition={SPRINGS.fast}
    >
      {icon && <span className="btn-icon">{icon}</span>}
      <span>{children}</span>
    </motion.button>
  );
});

// ---------- كرت ----------
export const Card = memo(function Card({
  title,
  icon,
  children,
  className = "",
}: {
  title?: string;
  icon?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <motion.section
      className={"card " + className}
      // رفع خفيف عند المرور (بلا مبالغة)
      whileHover={{ y: -2 }}
      transition={SPRINGS.page}
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
});

// ---------- صف (بأسلوب الإعدادات) ----------
export const Row = memo(function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children?: ReactNode;
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
});

// ---------- المقاطع (Segmented) ----------
function SegmentedBase<T extends string>({
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
                transition={SPRINGS.fast}
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

/** memo مع الحفاظ على النوع العام (generic) */
export const Segmented = memo(SegmentedBase) as typeof SegmentedBase;

// ---------- صف حالة (نقطة + نص) ----------
export const StatusRow = memo(function StatusRow({
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
  const dotClass =
    state === "ok" ? "ok" : state === "busy" ? "busy" : state === "na" ? "na" : "off";

  return (
    <div className="status-row">
      <span className="status-label">{label}</span>
      <span className="status-value">
        <span className={"dot " + dotClass} />
        {text}
      </span>
    </div>
  );
});

// ---------- سطر معلومة في كرت الجهاز ----------
export const DeviceRow = memo(function DeviceRow({ k, v }: { k: string; v: string }) {
  return (
    <div className="device-row">
      <span className="device-key">{k}</span>
      <span className="device-val" title={v}>
        {v}
      </span>
    </div>
  );
});

// ---------- الحالة الفاضية ----------
export const Empty = memo(function Empty({
  icon,
  title,
  desc,
  action,
}: {
  icon: ReactNode;
  title: string;
  desc: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <span className="empty-ico">{icon}</span>
      <h2>{title}</h2>
      <p className="muted">{desc}</p>
      {action}
    </div>
  );
});

// ---------- شريط التقدّم ----------
export const ProgressBar = memo(function ProgressBar({
  value,
}: {
  /** 0..1 = نسبة محددة · سالب = غير محدد (شريط يتحرك مثل أشرطة آبل) */
  value: number;
}) {
  const indeterminate = value < 0;
  return (
    <div
      className={"bar" + (indeterminate ? " indeterminate" : "")}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={indeterminate ? undefined : Math.round(value * 100)}
    >
      {!indeterminate && (
        <i
          style={{
            width: `${Math.round(Math.min(1, Math.max(0, value)) * 100)}%`,
          }}
        />
      )}
    </div>
  );
});
