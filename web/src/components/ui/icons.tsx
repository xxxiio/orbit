import type { SVGProps } from 'react'

export type IconProps = SVGProps<SVGSVGElement> & { size?: number }

function Icon({ size = 18, children, ...props }: IconProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...props}
    >
      {children}
    </svg>
  )
}

export function HomeIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4.5 10.5 12 4l7.5 6.5V19a1 1 0 0 1-1 1h-4.25v-5.25h-4.5V20H5.5a1 1 0 0 1-1-1z" />
    </Icon>
  )
}

export function InboxIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4 13.5V18a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4.5" />
      <path d="M4 13.5h4l1.5 2.25h5L16 13.5h4" />
      <path d="m5.6 5.4 1.2 4.9A2 2 0 0 0 8.75 12h6.5a2 2 0 0 0 1.95-1.7l1.2-4.9A1.5 1.5 0 0 0 16.95 3.5H7.05a1.5 1.5 0 0 0-1.45 1.9Z" />
    </Icon>
  )
}

export function FolderIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M3.5 7.5a2 2 0 0 1 2-2h3.2l1.8 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-12a2 2 0 0 1-2-2z" />
    </Icon>
  )
}

export function AgentsIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 3.5v3" />
      <rect x="5" y="6.5" width="14" height="11" rx="3.5" />
      <path d="M9.25 11.75h.01M14.75 11.75h.01" strokeWidth={2.4} />
      <path d="M9.5 15.25h5" />
      <path d="M2.75 11.5v3M21.25 11.5v3" />
    </Icon>
  )
}

export function SearchIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="10.75" cy="10.75" r="6.25" />
      <path d="m15.5 15.5 4 4" />
    </Icon>
  )
}

export function PlusIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 5v14M5 12h14" />
    </Icon>
  )
}

export function MenuIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4 7h16M4 12h16M4 17h16" />
    </Icon>
  )
}

export function CloseIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m6 6 12 12M18 6 6 18" />
    </Icon>
  )
}

export function SunIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2.75v2M12 19.25v2M4.6 4.6l1.4 1.4M18 18l1.4 1.4M2.75 12h2M19.25 12h2M4.6 19.4 6 18M18 6l1.4-1.4" />
    </Icon>
  )
}

export function MoonIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M20 14.4A8.25 8.25 0 0 1 9.6 4a8.25 8.25 0 1 0 10.4 10.4Z" />
    </Icon>
  )
}

export function MonitorIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="3.5" y="4.5" width="17" height="12" rx="2" />
      <path d="M9 20h6M12 16.5V20" />
    </Icon>
  )
}

export function ChevronDownIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m6 9.5 6 6 6-6" />
    </Icon>
  )
}

export function CheckIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m5 12.5 4.5 4.5L19 7.5" />
    </Icon>
  )
}

export function AlertCircleIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="8.75" />
      <path d="M12 7.75v5M12 16.1h.01" strokeWidth={2.2} />
    </Icon>
  )
}

export function AlertTriangleIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M10.3 4.9a2 2 0 0 1 3.4 0l7 12.1a2 2 0 0 1-1.7 3H5a2 2 0 0 1-1.7-3z" />
      <path d="M12 9v4.1M12 16.5h.01" strokeWidth={2.2} />
    </Icon>
  )
}

export function CheckCircleIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="8.75" />
      <path d="m8.25 12.25 2.5 2.5 5-5.5" />
    </Icon>
  )
}

export function LogOutIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M14 5.5h3.5a1.5 1.5 0 0 1 1.5 1.5v10a1.5 1.5 0 0 1-1.5 1.5H14" />
      <path d="M10 8.5 6 12l4 3.5M6.5 12H15" />
    </Icon>
  )
}

export function ArrowRightIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4.5 12h15M14 6.5l5.5 5.5L14 17.5" />
    </Icon>
  )
}

export function ArrowLeftIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M19.5 12h-15M10 6.5 4.5 12 10 17.5" />
    </Icon>
  )
}

export function CommandIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M9 6.5a2.5 2.5 0 1 0-2.5 2.5H9m0 0V9m0-2.5V9m0 0H6.5A2.5 2.5 0 1 0 9 11.5m0-2.5h6m-6 0V9m6 2.5a2.5 2.5 0 1 0 2.5-2.5H15m0 0V9m0-2.5V9m0 2.5V9m0 2.5h2.5A2.5 2.5 0 1 0 15 14a2.5 2.5 0 0 1-2.5 2.5H9" />
    </Icon>
  )
}

export function LoaderIcon(props: IconProps) {
  return (
    <Icon {...props} className={props.className ?? 'animate-spin'}>
      <path d="M12 3.5a8.5 8.5 0 1 0 8.5 8.5" />
    </Icon>
  )
}

export function DotIcon(props: IconProps) {
  return (
    <Icon {...props} strokeWidth={0} fill="currentColor">
      <circle cx="12" cy="12" r="3.25" />
    </Icon>
  )
}

export function KeyIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="8.25" cy="15.75" r="3.75" />
      <path d="m11 13 7.5-7.5M16 8l2 2M18.5 5.5l2 2" />
    </Icon>
  )
}

export function EyeIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M2.75 12S6 5.75 12 5.75 21.25 12 21.25 12 18 18.25 12 18.25 2.75 12 2.75 12Z" />
      <circle cx="12" cy="12" r="2.75" />
    </Icon>
  )
}

export function EyeOffIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4 4.5 20 20" />
      <path d="M9.9 6.1A9.6 9.6 0 0 1 12 5.75c6 0 9.25 6.25 9.25 6.25a15.8 15.8 0 0 1-3.2 4.05M6.4 7.7A16.3 16.3 0 0 0 2.75 12S6 18.25 12 18.25a9.3 9.3 0 0 0 3.6-.7" />
      <path d="M9.9 9.95a2.9 2.9 0 0 0 4.15 4.1" />
    </Icon>
  )
}

export function CalendarIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="3.75" y="5.5" width="16.5" height="14.5" rx="2.5" />
      <path d="M3.75 10h16.5M8 3.5v4M16 3.5v4" />
    </Icon>
  )
}

export function ClockIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7.5V12l3 2" />
    </Icon>
  )
}

export function GridIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="4" y="4" width="6.5" height="6.5" rx="1.5" />
      <rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5" />
      <rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5" />
      <rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5" />
    </Icon>
  )
}

export function ListIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M8.5 6.5h11.5M8.5 12h11.5M8.5 17.5h11.5" />
      <path d="M4.5 6.5h.01M4.5 12h.01M4.5 17.5h.01" strokeWidth={2.4} />
    </Icon>
  )
}

export function ChevronLeftIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m14.5 6-6 6 6 6" />
    </Icon>
  )
}

export function ChevronRightIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m9.5 6 6 6-6 6" />
    </Icon>
  )
}

export function RefreshIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M20 12a8 8 0 1 1-2.6-5.9" />
      <path d="M20 4.5V9h-4.5" />
    </Icon>
  )
}

export function ArrowUpIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 19.5v-15M6.5 10 12 4.5 17.5 10" />
    </Icon>
  )
}

export function ArrowDownIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 4.5v15M6.5 14 12 19.5 17.5 14" />
    </Icon>
  )
}


export function LinkIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M10.5 13.5a3.5 3.5 0 0 0 5 0l3-3a3.5 3.5 0 0 0-5-5l-1.5 1.5" />
      <path d="M13.5 10.5a3.5 3.5 0 0 0-5 0l-3 3a3.5 3.5 0 0 0 5 5l1.5-1.5" />
    </Icon>
  )
}

export function PinIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M9 4.5h6l-.75 5.25 2.25 2.25v1.5H7.5v-1.5L9.75 9.75z" />
      <path d="M12 13.5V20" />
    </Icon>
  )
}

export function LayersIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m12 4 8.5 4.5L12 13 3.5 8.5z" />
      <path d="m3.5 12.25 8.5 4.5 8.5-4.5M3.5 16l8.5 4.5 8.5-4.5" />
    </Icon>
  )
}

export function SettingsIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 15.25a3.25 3.25 0 1 0 0-6.5 3.25 3.25 0 0 0 0 6.5Z" />
      <path d="M19.4 13.5a7.7 7.7 0 0 0 0-3l1.6-1.2-1.8-3.1-1.9.7a7.6 7.6 0 0 0-2.6-1.5L14.4 3h-3.6l-.3 2.4a7.6 7.6 0 0 0-2.6 1.5l-1.9-.7L4.2 9.3 5.8 10.5a7.7 7.7 0 0 0 0 3L4.2 14.7l1.8 3.1 1.9-.7a7.6 7.6 0 0 0 2.6 1.5l.3 2.4h3.6l.3-2.4a7.6 7.6 0 0 0 2.6-1.5l1.9.7 1.8-3.1z" />
    </Icon>
  )
}
