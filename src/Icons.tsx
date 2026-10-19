// ============================================================
//  جسر — أيقونات خطّية بأسلوب SF Symbols (خطوط نظيفة بحواف مستديرة)
// ============================================================
import type { ReactElement, ReactNode } from "react";

export type IconProps = { size?: number; className?: string };

function Svg({
  size = 17,
  className,
  children,
}: IconProps & { children: ReactNode }): ReactElement {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

// شعار جسر: جسر بسيط (قوس + عمودين)
export const Bridge = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3 17.5h18" />
    <path d="M6.5 17.5V9" />
    <path d="M17.5 17.5V9" />
    <path d="M3.5 12.5c3-4.2 14-4.2 17 0" />
  </Svg>
);

export const Home = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3.8 10.6 12 3.8l8.2 6.8" />
    <path d="M6 9.9V19a1.2 1.2 0 0 0 1.2 1.2h2.6v-5.3h4.4v5.3h2.6A1.2 1.2 0 0 0 18 19V9.9" />
  </Svg>
);

export const Gamepad = (p: IconProps) => (
  <Svg {...p}>
    <rect x="2.6" y="7" width="18.8" height="10" rx="4.2" />
    <path d="M7.6 10.2v3.6M5.8 12h3.6" />
    <circle cx="16.2" cy="10.9" r="0.9" fill="currentColor" stroke="none" />
    <circle cx="18" cy="13.2" r="0.9" fill="currentColor" stroke="none" />
  </Svg>
);

export const Bottle = (p: IconProps) => (
  <Svg {...p}>
    <path d="M8.2 3.2h7.6v2.6c0 .7-.3 1.3-.8 1.7-.5.4-.8 1-.8 1.6v10a1.6 1.6 0 0 1-1.6 1.6h-2a1.6 1.6 0 0 1-1.6-1.6v-10c0-.6-.3-1.2-.8-1.6-.5-.4-.8-1-.8-1.7z" />
    <path d="M8.2 12.4h7.6" />
  </Svg>
);

export const Sliders = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 8h9M17 8h3M4 16h3M11 16h9" />
    <circle cx="15" cy="8" r="2" />
    <circle cx="9" cy="16" r="2" />
  </Svg>
);

export const Doc = (p: IconProps) => (
  <Svg {...p}>
    <path d="M7.2 3.4h6.4l4.2 4.2V19a1.6 1.6 0 0 1-1.6 1.6H7.2A1.6 1.6 0 0 1 5.6 19V5a1.6 1.6 0 0 1 1.6-1.6z" />
    <path d="M13.4 3.6v4.2h4.2" />
    <path d="M8.6 12.6h6.8M8.6 16h4.8" />
  </Svg>
);

export const Plus = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 5.2v13.6M5.2 12h13.6" />
  </Svg>
);

export const Download = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 4v10.4" />
    <path d="M7.8 10.6 12 14.8l4.2-4.2" />
    <path d="M5 19.4h14" />
  </Svg>
);

export const Chip = (p: IconProps) => (
  <Svg {...p}>
    <rect x="6.6" y="6.6" width="10.8" height="10.8" rx="2.4" />
    <path d="M9.6 2.8v3.4M14.4 2.8v3.4M9.6 17.8v3.4M14.4 17.8v3.4M2.8 9.6h3.4M2.8 14.4h3.4M17.8 9.6h3.4M17.8 14.4h3.4" />
  </Svg>
);

export const Sparkle = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3.4l1.7 5.1 5.1 1.7-5.1 1.7L12 17l-1.7-5.1L5.2 10.2l5.1-1.7z" />
    <path d="M18.6 16.4l.7 2 2 .7-2 .7-.7 2-.7-2-2-.7 2-.7z" />
  </Svg>
);

export const Check = (p: IconProps) => (
  <Svg {...p}>
    <path d="M5.2 12.6 9.8 17 18.8 7.4" />
  </Svg>
);

export const Play = (p: IconProps) => (
  <Svg {...p}>
    <path d="M8.4 5.6v12.8l10.4-6.4z" />
  </Svg>
);

export const Stop = (p: IconProps) => (
  <Svg {...p}>
    <rect x="6.4" y="6.4" width="11.2" height="11.2" rx="2.6" />
  </Svg>
);

export const Monitor = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3" y="4.6" width="18" height="12.2" rx="2.2" />
    <path d="M9 20h6M12 16.8V20" />
  </Svg>
);

export const Moon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M20 14.2A8.2 8.2 0 0 1 9.8 4a8.4 8.4 0 1 0 10.2 10.2z" />
  </Svg>
);

export const Sun = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2.6v2.2M12 19.2v2.2M2.6 12h2.2M19.2 12h2.2M5.4 5.4l1.6 1.6M17 17l1.6 1.6M18.6 5.4 17 7M7 17l-1.6 1.6" />
  </Svg>
);

export const Globe = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="8.4" />
    <path d="M3.6 12h16.8" />
    <path d="M12 3.6c2.4 2.6 3.6 5.4 3.6 8.4s-1.2 5.8-3.6 8.4c-2.4-2.6-3.6-5.4-3.6-8.4s1.2-5.8 3.6-8.4z" />
  </Svg>
);

export const Copy = (p: IconProps) => (
  <Svg {...p}>
    <rect x="8.6" y="8.6" width="10.8" height="10.8" rx="2.2" />
    <path d="M15.4 5.4A1.8 1.8 0 0 0 13.6 3.6H6.4A2.8 2.8 0 0 0 3.6 6.4v7.2c0 .9.7 1.6 1.6 1.8" />
  </Svg>
);

export const Folder = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3.6 7.4c0-1 .8-1.8 1.8-1.8h3.5l1.7 1.9h8c1 0 1.8.8 1.8 1.8v6.9c0 1-.8 1.8-1.8 1.8H5.4c-1 0-1.8-.8-1.8-1.8z" />
  </Svg>
);

export const Trash = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4.6 6.8h14.8" />
    <path d="M9.4 6.8V5.2c0-.9.7-1.6 1.6-1.6h2c.9 0 1.6.7 1.6 1.6v1.6" />
    <path d="M6.6 6.8l.9 12a1.6 1.6 0 0 0 1.6 1.5h5.8a1.6 1.6 0 0 0 1.6-1.5l.9-12" />
  </Svg>
);

export const Bolt = (p: IconProps) => (
  <Svg {...p}>
    <path d="M13.4 2.8 5.8 13.4h5l-.6 7.8 7.6-10.6h-5z" />
  </Svg>
);
