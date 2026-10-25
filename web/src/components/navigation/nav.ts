import type { ComponentType } from 'react'
import { AgentsIcon, FolderIcon, HomeIcon, InboxIcon, type IconProps } from '@/components/ui/icons'

export interface GlobalNavItem {
  to: string
  label: string
  icon: ComponentType<IconProps>
  end?: boolean
}

export const globalNav: GlobalNavItem[] = [
  { to: '/', label: 'Home', icon: HomeIcon, end: true },
  { to: '/inbox', label: 'Inbox', icon: InboxIcon },
  { to: '/projects', label: 'Projects', icon: FolderIcon },
  { to: '/agents', label: 'Agents', icon: AgentsIcon },
]

/**
 * The sidebar owns the shape of a project link, but not the data.
 * A later lane fills `projects` from the server; until then the shell renders
 * an explicit empty state.
 */
export interface ProjectNavItem {
  id: string
  name: string
  pinned?: boolean
  /** Opened recently in this browser; not server state. */
  recent?: boolean
}
