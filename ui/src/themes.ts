// Six reading backgrounds shared with LNC Library (uilib/themes.ts there), so the
// shelf and the reader sit in the same light. The server keeps the same identifier
// list in src/themes.rs and validates `default_reader_mode` against it.
//
// `data-mode` on <html> carries the tokens (style.css); `theme-color` paints the
// phone status bar and must follow the same six values.

export type ReaderMode = 'paper' | 'sepia' | 'gray' | 'green' | 'night' | 'black'

export type ThemeSpec = {
  id: ReaderMode
  vi: string
  en: string
  /** page background — also the browser/status-bar colour */
  bg: string
  dark: boolean
}

export const THEMES: ThemeSpec[] = [
  { id: 'paper', vi: 'Giấy', en: 'Paper', bg: '#faf8f5', dark: false },
  { id: 'sepia', vi: 'Sepia', en: 'Sepia', bg: '#f3ead8', dark: false },
  { id: 'gray', vi: 'Xám dịu', en: 'Soft grey', bg: '#ededeb', dark: false },
  { id: 'green', vi: 'Xanh dịu', en: 'Soft green', bg: '#e4ede1', dark: false },
  { id: 'night', vi: 'Đêm', en: 'Night', bg: '#15171a', dark: true },
  { id: 'black', vi: 'Đen', en: 'Black', bg: '#090909', dark: true },
]

export const MODE_IDS = THEMES.map((theme) => theme.id)

export function isMode(value: unknown): value is ReaderMode {
  return typeof value === 'string' && (MODE_IDS as string[]).includes(value)
}

export function themeSpec(id: ReaderMode): ThemeSpec {
  return THEMES.find((theme) => theme.id === id) ?? THEMES[0]
}

/** Paint the document root: mode tokens, color-scheme and the phone status bar. */
export function applyMode(id: ReaderMode) {
  const spec = themeSpec(id)
  document.documentElement.dataset.mode = spec.id
  document.documentElement.style.colorScheme = spec.dark ? 'dark' : 'light'
  let meta = document.querySelector('meta[name="theme-color"]') as HTMLMetaElement | null
  if (!meta) {
    meta = document.createElement('meta')
    meta.name = 'theme-color'
    document.head.append(meta)
  }
  meta.content = spec.bg
}
