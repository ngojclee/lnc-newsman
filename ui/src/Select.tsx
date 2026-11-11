import { KeyboardEvent, useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'

export type SelectOption = { value: string; label: string; hint?: string; disabled?: boolean }
type Props = { label: string; value: string; options: SelectOption[]; onChange: (value: string) => void; hint?: string; disabled?: boolean }
type Placement = { top: number; left: number; width: number; maxHeight: number; above: boolean }

// Select-only combobox. The menu is portalled into the themed shell with position: fixed,
// so cards with overflow cannot clip it, and it keeps the trigger's corner radius.
export function Select({ label, value, options, onChange, hint, disabled }: Props) {
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
  const selectedIndex = Math.max(0, options.findIndex((option) => option.value === value))
  const selected = options[selectedIndex]

  const place = () => {
    const rect = trigger.current?.getBoundingClientRect()
    if (!rect) return
    const below = window.innerHeight - rect.bottom - 12
    const above = rect.top - 12
    const wanted = Math.min(300, options.length * 42 + 12)
    const up = below < wanted && above > below
    setPlacement({ left: rect.left, width: rect.width, top: up ? rect.top - 6 : rect.bottom + 6, maxHeight: Math.max(120, Math.min(300, up ? above : below)), above: up })
  }
  useLayoutEffect(() => { if (open) place() }, [open])
  useEffect(() => {
    if (!open) return
    const onScroll = (event: Event) => { if (!menu.current?.contains(event.target as Node)) place() }
    const onPointer = (event: PointerEvent) => {
      const target = event.target as Node
      if (!trigger.current?.contains(target) && !menu.current?.contains(target)) setOpen(false)
    }
    window.addEventListener('scroll', onScroll, true)
    window.addEventListener('resize', place)
    window.addEventListener('pointerdown', onPointer)
    return () => { window.removeEventListener('scroll', onScroll, true); window.removeEventListener('resize', place); window.removeEventListener('pointerdown', onPointer) }
  }, [open])
  useEffect(() => { if (open) menu.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' }) }, [open, active])

  const show = () => { setActive(selectedIndex); setOpen(true) }
  const choose = (index: number) => {
    const option = options[index]
    if (!option || option.disabled) return
    if (option.value !== value) onChange(option.value)
    setOpen(false)
    trigger.current?.focus()
  }
  const step = (from: number, delta: number) => {
    let index = from
    for (let n = 0; n < options.length; n++) {
      index = (index + delta + options.length) % options.length
      if (!options[index].disabled) return index
    }
    return from
  }
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const { key } = event
    if (!open) {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(key)) { event.preventDefault(); show() }
      return
    }
    if (key === 'ArrowDown') { event.preventDefault(); setActive((i) => step(i, 1)) }
    else if (key === 'ArrowUp') { event.preventDefault(); setActive((i) => step(i, -1)) }
    else if (key === 'Home') { event.preventDefault(); setActive(step(-1, 1)) }
    else if (key === 'End') { event.preventDefault(); setActive(step(options.length, -1)) }
    else if (key === 'Enter' || key === ' ') { event.preventDefault(); choose(active) }
    else if (key === 'Escape') { event.preventDefault(); setOpen(false) }
    else if (key === 'Tab') { setOpen(false) }
    else if (key.length === 1) {
      const now = Date.now()
      typed.current = { text: now - typed.current.at > 700 ? key.toLowerCase() : typed.current.text + key.toLowerCase(), at: now }
      const match = options.findIndex((option) => !option.disabled && option.label.toLowerCase().startsWith(typed.current.text))
      if (match >= 0) setActive(match)
    }
  }
  const host = trigger.current?.closest<HTMLElement>('.app-shell, .login-page') ?? document.body

  return <div className="field">
    <span id={labelId} onClick={() => trigger.current?.focus()}>{label}</span>
    <button ref={trigger} type="button" role="combobox" className="select-trigger" disabled={disabled}
      aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? listId : undefined}
      aria-labelledby={`${labelId} ${valueId}`} aria-activedescendant={open ? `${id}-option-${active}` : undefined}
      onClick={() => (open ? setOpen(false) : show())} onKeyDown={onKeyDown}>
      <span id={valueId} className="select-value">{selected?.label ?? ''}</span>
      <svg className="select-chevron" aria-hidden="true" viewBox="0 0 16 16" width="16" height="16"><path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" /></svg>
    </button>
    {hint && <small>{hint}</small>}
    {open && placement && createPortal(
      <ul ref={menu} id={listId} role="listbox" aria-labelledby={labelId} className={placement.above ? 'select-menu above' : 'select-menu'}
        style={{ left: placement.left, width: placement.width, maxHeight: placement.maxHeight, ...(placement.above ? { bottom: window.innerHeight - placement.top } : { top: placement.top }) }}>
        {options.map((option, index) => <li key={option.value} id={`${id}-option-${index}`} data-index={index} role="option"
          aria-selected={option.value === value} aria-disabled={option.disabled || undefined}
          className={index === active ? 'select-option active' : 'select-option'}
          onPointerMove={() => !option.disabled && index !== active && setActive(index)} onPointerDown={(event) => event.preventDefault()} onClick={() => choose(index)}>
          <span>{option.label}{option.hint && <small>{option.hint}</small>}</span>{option.value === value && <span className="select-check" aria-hidden="true">✓</span>}
        </li>)}
      </ul>, host)}
  </div>
}
