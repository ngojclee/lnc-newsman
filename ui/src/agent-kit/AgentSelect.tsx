import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

export interface AgentSelectOption {
  value: string;
  label: string;
  badge?: string;
}

export interface AgentSelectProps {
  value: string;
  options: AgentSelectOption[];
  onChange: (value: string) => void;
  size?: "md" | "sm";
  disabled?: boolean;
  ariaLabel?: string;
  className?: string;
}

type PortalRect = { left: number; top: number; minWidth: number };

/**
 * Custom rounded droplist for `lnc-app-agents v2.0`.
 * - Never uses native browser OS dropdown primitives.
 * - Portals into `.app-shell, .reader-shell, .reader, .login-page` (fallback `document.body`)
 *   so Newsman Paper/Sepia/Gray/Green/Night/Black theme CSS variables stay inherited.
 */
export function AgentSelect({
  value,
  options,
  onChange,
  size = "md",
  disabled = false,
  ariaLabel,
  className = "",
}: AgentSelectProps) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [rect, setRect] = useState<PortalRect | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const selectedIndex = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  );
  const current = options[selectedIndex] ?? options[0];

  const computeRect = () => {
    const el = triggerRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const estHeight = Math.min(280, Math.max(1, options.length) * 34 + 12);
    const shouldFlip =
      window.innerHeight - r.bottom < estHeight + 10 && r.top > estHeight + 10;
    const minWidth = Math.max(r.width, 180);
    const maxLeft = Math.max(8, window.innerWidth - minWidth - 8);
    setRect({
      left: Math.min(r.left, maxLeft),
      top: shouldFlip ? r.top - estHeight - 4 : r.bottom + 4,
      minWidth,
    });
  };

  useLayoutEffect(() => {
    if (open) computeRect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, options.length]);

  useEffect(() => {
    if (!open) return;
    const sync = () => computeRect();
    window.addEventListener("scroll", sync, true);
    window.addEventListener("resize", sync);
    const onOutside = (e: MouseEvent) => {
      const t = e.target as Node;
      if (triggerRef.current?.contains(t) || panelRef.current?.contains(t)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", onOutside);
    return () => {
      window.removeEventListener("scroll", sync, true);
      window.removeEventListener("resize", sync);
      document.removeEventListener("mousedown", onOutside);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const commit = (nextVal: string) => {
    onChange(nextVal);
    setOpen(false);
    triggerRef.current?.focus();
  };

  const portalHost =
    typeof document !== "undefined"
      ? (document.querySelector(".app-shell, .reader-shell, .reader, .login-page") ?? document.body)
      : null;

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => {
          if (!open) setActive(selectedIndex);
          setOpen(!open);
        }}
        onKeyDown={(e) => {
          if (!open && (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ")) {
            e.preventDefault();
            setActive(selectedIndex);
            setOpen(true);
          } else if (open && e.key === "Escape") {
            e.preventDefault();
            setOpen(false);
          } else if (open && e.key === "ArrowDown") {
            e.preventDefault();
            setActive((i) => Math.min(options.length - 1, i + 1));
          } else if (open && e.key === "ArrowUp") {
            e.preventDefault();
            setActive((i) => Math.max(0, i - 1));
          } else if (open && (e.key === "Enter" || e.key === " ")) {
            e.preventDefault();
            if (options[active]) commit(options[active].value);
          }
        }}
        className={`engine-select-trigger ak-select-trigger ${
          size === "sm" ? "ak-select-sm" : ""
        }${open ? " open" : ""} ${className}`.trim()}
      >
        <span className="engine-select-value ak-truncate">{current?.label ?? ""}</span>
        {current?.badge ? <span className="ak-select-badge">{current.badge}</span> : null}
        <span className="engine-select-arrow" aria-hidden="true" />
      </button>

      {open && rect && portalHost
        ? createPortal(
            <div
              ref={panelRef}
              role="listbox"
              className="engine-select-panel engine-select-panel--portal ak-select-panel"
              style={{ left: rect.left, top: rect.top, minWidth: rect.minWidth }}
            >
              {options.map((opt, idx) => (
                <button
                  key={opt.value}
                  type="button"
                  role="option"
                  aria-selected={opt.value === value}
                  onMouseEnter={() => setActive(idx)}
                  onClick={() => commit(opt.value)}
                  className={`engine-select-option ak-select-option${
                    opt.value === value ? " selected" : ""
                  }${idx === active ? " is-active" : ""}`}
                >
                  <span className="ak-truncate">{opt.label}</span>
                  {opt.badge ? <span className="ak-select-badge">{opt.badge}</span> : null}
                </button>
              ))}
            </div>,
            portalHost,
          )
        : null}
    </>
  );
}

export default AgentSelect;
