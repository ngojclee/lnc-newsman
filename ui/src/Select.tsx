import { type KeyboardEvent, type ReactNode, useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'

export type SelectOption = { value: string; label: string; hint?: string; disabled?: boolean }

export type SelectProps = {
  label?: string
  ariaLabel?: string
  value: string
  options: SelectOption[]
  onChange: (value: string) => void
  hint?: string
  disabled?: boolean
  variant?: 'field' | 'pill' | 'compact'
  leadingIcon?: ReactNode
  minMenuWidth?: number
  className?: string
}

type Placement = { top: number; left: number; width: number; maxHeight: number; above: boolean }

// Select-only combobox. The menu is portalled into the themed shell with position: fixed,
// so cards and modals with overflow cannot clip it, and it keeps theme CSS custom properties.
export function Select({
  label,
  ariaLabel,
  value,
  options,
  onChange,
  hint,
  disabled,
  variant = 'field',
  leadingIcon,
  minMenuWidth,
  className,
}: SelectProps) {
  const id = useId()
  const labelId = `${id}-label`
  const valueId = `${id}-value`
  const listId = `${id}-list`
  const trigger = useRef<HTMLButtonElement>(null)
  const menu = useRef<HTMLUListElement>(null)
  const typed = useRef({ text: '', at: 0 })
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const [placement, setPlacement] = useState<Placement | null>(null)
  const selectedIndex = Math.max(
    0,
    options.findIndex((option) => option.value === value),
  )
  const selected = options[selectedIndex]

  const place = () => {
    const rect = trigger.current?.getBoundingClientRect()
    if (!rect) return
    const defaultMin = variant === 'pill' || variant === 'compact' ? 210 : 0
    const rawWidth = Math.max(rect.width, minMenuWidth ?? defaultMin)
    const menuWidth = Math.min(rawWidth, Math.max(140, window.innerWidth - 24))
    const maxLeft = Math.max(12, window.innerWidth - menuWidth - 12)
    const left = Math.min(Math.max(12, rect.left), maxLeft)
    const below = window.innerHeight - rect.bottom - 12
    const above = rect.top - 12
    const wanted = Math.min(300, Math.max(132, options.length * 42 + 12))
    const up = below < wanted && above > below
    setPlacement({
      left,
      width: menuWidth,
      top: up ? rect.top - 6 : rect.bottom + 6,
      maxHeight: Math.max(120, Math.min(300, up ? above : below)),
      above: up,
    })
  }

  useLayoutEffect(() => {
    if (open) place()
  }, [open, options.length, minMenuWidth, variant])

  useEffect(() => {
    if (!open) return
    const onScroll = (event: Event) => {
      if (!menu.current?.contains(event.target as Node)) place()
    }
    const onPointer = (event: PointerEvent) => {
      const target = event.target as Node
      if (!trigger.current?.contains(target) && !menu.current?.contains(target)) setOpen(false)
    }
    window.addEventListener('scroll', onScroll, true)
    window.addEventListener('resize', place)
    window.addEventListener('pointerdown', onPointer)
    return () => {
      window.removeEventListener('scroll', onScroll, true)
      window.removeEventListener('resize', place)
      window.removeEventListener('pointerdown', onPointer)
    }
  }, [open])

  useEffect(() => {
    if (open) {
      menu.current
        ?.querySelector<HTMLElement>(`[data-index="${active}"]`)
        ?.scrollIntoView({ block: 'nearest' })
    }
  }, [open, active])

  const show = () => {
    if (disabled || options.length === 0) return
    setActive(selectedIndex)
    setOpen(true)
  }

  const choose = (index: number) => {
    const option = options[index]
    if (!option || option.disabled) return
    if (option.value !== value) onChange(option.value)
    setOpen(false)
    trigger.current?.focus()
  }

  const step = (from: number, delta: number) => {
    if (options.length === 0) return 0
    let index = from
    for (let n = 0; n < options.length; n++) {
      index = (index + delta + options.length) % options.length
      if (!options[index]?.disabled) return index
    }
    return from
  }

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const { key } = event
    if (!open) {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(key)) {
        event.preventDefault()
        show()
      }
      return
    }
    if (key === 'ArrowDown') {
      event.preventDefault()
      setActive((i) => step(i, 1))
    } else if (key === 'ArrowUp') {
      event.preventDefault()
      setActive((i) => step(i, -1))
    } else if (key === 'Home') {
      event.preventDefault()
      setActive(step(-1, 1))
    } else if (key === 'End') {
      event.preventDefault()
      setActive(step(options.length, -1))
    } else if (key === 'Enter' || key === ' ') {
      event.preventDefault()
      choose(active)
    } else if (key === 'Escape') {
      event.preventDefault()
      setOpen(false)
    } else if (key === 'Tab') {
      setOpen(false)
    } else if (key.length === 1) {
      const now = Date.now()
      typed.current = {
        text: now - typed.current.at > 700 ? key.toLowerCase() : typed.current.text + key.toLowerCase(),
        at: now,
      }
      const match = options.findIndex(
        (option) => !option.disabled && option.label.toLowerCase().startsWith(typed.current.text),
      )
      if (match >= 0) setActive(match)
    }
  }

  const host =
    trigger.current?.closest<HTMLElement>('.app-shell, .reader-shell, .reader, .login-page') ??
    document.body

  const hasLabel = Boolean(label && label.trim().length > 0)
  const accessibleName = ariaLabel || label || selected?.label || 'Select'
  const wrapperClass = [
    hasLabel || variant === 'field' ? 'field' : 'select-inline',
    variant === 'compact' && hasLabel ? 'field--compact' : '',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ')

  const triggerClass = [
    'select-trigger',
    variant === 'pill' ? 'select-trigger--pill' : '',
    variant === 'compact' ? 'select-trigger--compact' : '',
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <div className={wrapperClass}>
      {hasLabel && (
        <span id={labelId} onClick={() => trigger.current?.focus()}>
          {label}
        </span>
      )}
      <button
        ref={trigger}
        type="button"
        role="combobox"
        className={triggerClass}
        disabled={disabled}
        title={!hasLabel && ariaLabel ? ariaLabel : undefined}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-labelledby={hasLabel ? `${labelId} ${valueId}` : undefined}
        aria-label={!hasLabel ? accessibleName : undefined}
        aria-activedescendant={open ? `${id}-option-${active}` : undefined}
        onClick={() => (open ? setOpen(false) : show())}
        onKeyDown={onKeyDown}
      >
        {leadingIcon !== undefined && leadingIcon !== null && (
          <span className="select-leading" aria-hidden="true">
            {leadingIcon}
          </span>
        )}
        <span id={valueId} className="select-value">
          {selected?.label ?? ''}
        </span>
        <svg className="select-chevron" aria-hidden="true" viewBox="0 0 16 16" width="16" height="16">
          <path
            d="M4 6l4 4 4-4"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      {hint && <small>{hint}</small>}
      {open &&
        placement &&
        createPortal(
          <ul
            ref={menu}
            id={listId}
            role="listbox"
            aria-labelledby={hasLabel ? labelId : undefined}
            aria-label={!hasLabel ? accessibleName : undefined}
            className={placement.above ? 'select-menu above' : 'select-menu'}
            style={{
              left: placement.left,
              width: placement.width,
              maxHeight: placement.maxHeight,
              ...(placement.above ? { bottom: window.innerHeight - placement.top } : { top: placement.top }),
            }}
          >
            {options.map((option, index) => (
              <li
                key={option.value}
                id={`${id}-option-${index}`}
                data-index={index}
                role="option"
                aria-selected={option.value === value}
                aria-disabled={option.disabled || undefined}
                className={index === active ? 'select-option active' : 'select-option'}
                onPointerMove={() => !option.disabled && index !== active && setActive(index)}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => choose(index)}
              >
                <span>
                  {option.label}
                  {option.hint && <small>{option.hint}</small>}
                </span>
                {option.value === value && (
                  <span className="select-check" aria-hidden="true">
                    ✓
                  </span>
                )}
              </li>
            ))}
          </ul>,
          host,
        )}
    </div>
  )
}
