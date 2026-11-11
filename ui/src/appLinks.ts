// Shared app switcher for the lengoc.me internal apps.
//
// Decision (2026-11-11): each app keeps its own origin and session; the
// switch is a plain link row, not a shared-session mechanism. See
// lnc-library/.docs/unified-access.md for why (SPAs use absolute paths, so a
// single domain with /news + /library would need base-path support in both).
//
// Keep this file byte-identical in lnc-newsman and lnc-library.

export type AppLink = { id: 'newsman' | 'library'; label: string; href: string; icon: string }

const ORIGINS = {
  newsman: 'https://newsman.lengoc.me',
  library: 'https://ebooks.lengoc.me',
} as const

export const APP_LINKS: AppLink[] = [
  { id: 'newsman', label: 'Newsman', href: `${ORIGINS.newsman}/`, icon: '◈' },
  { id: 'library', label: 'Library', href: `${ORIGINS.library}/`, icon: '▤' },
]

/**
 * Local development override: when running from 127.0.0.1, point the links at
 * the sibling dev servers instead of production so the switcher is usable
 * while working on either app.
 */
export function appLinks(current: AppLink['id'], local = false): AppLink[] {
  const localPorts: Record<AppLink['id'], string> = {
    newsman: 'http://127.0.0.1:8000/',
    library: 'http://127.0.0.1:8080/',
  }
  return APP_LINKS.map((link) => ({
    ...link,
    href: local && link.id !== current ? localPorts[link.id] : link.href,
  }))
}
