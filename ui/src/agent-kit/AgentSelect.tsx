import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { foldVi } from "./agentTypesV2";

export type ProviderStatusDot = "connected" | "connecting" | "error" | "disabled";

export interface AgentSelectOption {
  value: string;
  label: string;
  badge?: string;
  dots?: ProviderStatusDot[];
  keywords?: string;
}

export interface AgentSelectProps {
  value: string;
  options: AgentSelectOption[];
  onChange: (value: string) => void;
  size?: "md" | "sm";
  disabled?: boolean;
  ariaLabel?: string;
  className?: string;
  /** Default `"auto"` enables inline search when `options.length > 6`. */
  searchable?: boolean | "auto";
  searchPlaceholder?: string;
  /** Default `true`: filters via `foldVi()` (Vietnamese accent-insensitive). */
  fold?: boolean;
}

type PortalRect = {
  left: number;
  top?: number;
  bottom?: number;
  minWidth: number;
  maxHeight: number;
  above: boolean;
};

/**
 * Custom rounded searchable combobox for `lnc-app-agents v2.2`.
 * - Never renders native OS dropdown primitives.
 * - Portals to `document.body` so overflow containers never clip it.
 * - Preserves `5fde443` hidden-trigger auto-close + `hashchange` / `popstate` / `Escape` guards,
 *   while adding `foldVi()` search (`[data-tray-search]`) and provider status `dots`.
 */
export function AgentSelect({
  value,
  options,
  onChange,
  size = "md",
  disabled = false,
  ariaLabel,
  className = "",
  searchable = "auto",
  searchPlaceholder = "Search...",
  fold = true,
}: AgentSelectProps) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [query, setQuery] = useState("");
  const [rect, setRect] = useState<PortalRect | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  const isSearchable =
    searchable === true || (searchable !== false && options.length > 6);

  const visibleOptions = useMemo(() => {
    const q = fold ? foldVi(query) : query.trim().toLowerCase();
    if (!isSearchable || !q) return options;
    return options.filter((o) => {
      const raw = `${o.label} ${o.value} ${o.badge ?? ""} ${o.keywords ?? ""}`;
      return (fold ? foldVi(raw) : raw.toLowerCase()).includes(q);
    });
  }, [options, isSearchable, query, fold]);

  const selectedIndex = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  );
  const current = options[selectedIndex] ?? options[0];

  const computeRect = () => {
    const el = triggerRef.current;
    if (!el || !el.isConnected || el.offsetParent === null) {
      setOpen(false);
      return;
    }
    const r = el.getBoundingClientRect();
    const SAFE_MARGIN = 32;
    const GAP = 6;
    const rowCount = Math.max(1, visibleOptions.length) + (isSearchable ? 1 : 0);
    const estHeight = Math.min(310, Math.max(88, rowCount * 36 + 16));
    const spaceBelow = window.innerHeight - r.bottom - SAFE_MARGIN;
    const spaceAbove = r.top - 16;
    const openUpward =
      (spaceBelow < estHeight && spaceAbove > spaceBelow) ||
      r.bottom > window.innerHeight - 190;
    const minWidth = Math.max(Math.ceil(r.width), 184);
    const panelWidth = panelRef.current?.offsetWidth || minWidth;
    const clampedLeft = Math.max(
      12,
      Math.min(Math.round(r.left), window.innerWidth - panelWidth - 12),
    );
    if (openUpward) {
      setRect({
        left: clampedLeft,
        bottom: Math.max(SAFE_MARGIN, Math.round(window.innerHeight - r.top + GAP)),
        minWidth,
        maxHeight: Math.max(120, Math.min(300, Math.floor(spaceAbove - GAP))),
        above: true,
      });
    } else {
      setRect({
        left: clampedLeft,
        top: Math.round(r.bottom + GAP),
        minWidth,
        maxHeight: Math.max(120, Math.min(300, Math.floor(spaceBelow - GAP))),
        above: false,
      });
    }
  };

  useLayoutEffect(() => {
    if (open) computeRect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, visibleOptions.length, isSearchable]);

  useEffect(() => {
    if (!open) {
      setQuery("");
      return;
    }
    const raf = isSearchable
      ? requestAnimationFrame(() => searchInputRef.current?.focus())
      : 0;
    const sync = () => computeRect();
    const onRouteOrHash = () => setOpen(false);
    window.addEventListener("scroll", sync, true);
    window.addEventListener("resize", sync);
    window.addEventListener("hashchange", onRouteOrHash);
    window.addEventListener("popstate", onRouteOrHash);
    const onOutside = (e: MouseEvent) => {
      const t = e.target as Node;
      if (triggerRef.current?.contains(t) || panelRef.current?.contains(t)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onOutside);
    window.addEventListener("keydown", onKey);
    return () => {
      if (raf) cancelAnimationFrame(raf);
      window.removeEventListener("scroll", sync, true);
      window.removeEventListener("resize", sync);
      window.removeEventListener("hashchange", onRouteOrHash);
      window.removeEventListener("popstate", onRouteOrHash);
      document.removeEventListener("mousedown", onOutside);
      window.removeEventListener("keydown", onKey);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, isSearchable]);

  useEffect(() => {
    setActive((cur) => Math.max(0, Math.min(cur, Math.max(0, visibleOptions.length - 1))));
  }, [visibleOptions.length]);

  const commit = (nextVal: string) => {
    onChange(nextVal);
    setOpen(false);
    triggerRef.current?.focus();
  };

  const portalHost = typeof document !== "undefined" ? document.body : null;

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
            setActive((i) => Math.min(visibleOptions.length - 1, i + 1));
          } else if (open && e.key === "ArrowUp") {
            e.preventDefault();
            setActive((i) => Math.max(0, i - 1));
          } else if (open && (e.key === "Enter" || e.key === " ")) {
            e.preventDefault();
            if (visibleOptions[active]) commit(visibleOptions[active].value);
          }
        }}
        className={`engine-select-trigger ak-select-trigger ${
          size === "sm" ? "ak-select-sm" : ""
        }${open ? " open" : ""} ${className}`.trim()}
      >
        <span className="engine-select-value ak-truncate">{current?.label ?? ""}</span>
        {current?.dots?.length ? (
          <span className="ak-status-dots" aria-hidden="true">
            {current.dots.map((d, di) => (
              <span key={di} className="ak-status-dot" data-status={d} />
            ))}
          </span>
        ) : null}
        {current?.badge ? <span className="ak-select-badge">{current.badge}</span> : null}
        <span className="engine-select-arrow" aria-hidden="true" />
      </button>

      {open && rect && portalHost
        ? createPortal(
            <div
              ref={panelRef}
              role="listbox"
              className={`engine-select-panel engine-select-panel--portal ak-select-panel${rect.above ? " above" : ""}`}
              style={{
                left: rect.left,
                ...(rect.bottom !== undefined ? { bottom: rect.bottom } : { top: rect.top }),
                minWidth: rect.minWidth,
                width: "max-content",
                maxWidth: "min(440px, calc(100vw - 24px))",
                maxHeight: rect.maxHeight,
              }}
            >
              {isSearchable ? (
                <div className="ak-select-search-wrap">
                  <input
                    ref={searchInputRef}
                    data-tray-search=""
                    type="search"
                    className="ak-select-search"
                    value={query}
                    onChange={(e) => {
                      setQuery(e.target.value);
                      setActive(0);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Escape") {
                        e.preventDefault();
                        setOpen(false);
                        triggerRef.current?.focus();
                      } else if (e.key === "ArrowDown") {
                        e.preventDefault();
                        setActive((i) => Math.min(visibleOptions.length - 1, i + 1));
                      } else if (e.key === "ArrowUp") {
                        e.preventDefault();
                        setActive((i) => Math.max(0, i - 1));
                      } else if (e.key === "Enter" && visibleOptions[active]) {
                        e.preventDefault();
                        commit(visibleOptions[active].value);
                      }
                    }}
                    placeholder={searchPlaceholder}
                    aria-label={searchPlaceholder}
                  />
                </div>
              ) : null}
              {visibleOptions.length === 0 ? (
                <div className="ak-select-empty">No matching options</div>
              ) : (
                visibleOptions.map((opt, idx) => (
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
                    {opt.dots?.length ? (
                      <span className="ak-status-dots" aria-hidden="true">
                        {opt.dots.map((d, di) => (
                          <span key={di} className="ak-status-dot" data-status={d} />
                        ))}
                      </span>
                    ) : null}
                    {opt.badge ? <span className="ak-select-badge">{opt.badge}</span> : null}
                  </button>
                ))
              )}
            </div>,
            portalHost,
          )
        : null}
    </>
  );
}

export default AgentSelect;
