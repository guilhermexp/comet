import { useId } from 'react'
import { cn } from '@/lib/utils'

/**
 * App-chrome icons shared, pixel-for-pixel, with the native macOS app.
 *
 * These are the exact SVGs from `apps/native/.../ChromeIcons.swift`; keep the
 * path data in sync with ChromeIcons.swift if either side changes.
 */

type IconProps = { className?: string }

/** Phosphor fill icon (256 viewBox), tinted by `currentColor`. */
function Phosphor({ d, className }: { d: string; className?: string }) {
  return (
    <svg viewBox="0 0 256 256" fill="currentColor" className={cn('shrink-0', className)} aria-hidden>
      <path d={d} />
    </svg>
  )
}

/** Tinted glass fill: translucent gradient + faint edge highlight. */
function GlassPhosphor({ d, className }: { d: string; className?: string }) {
  const id = useId().replace(/:/g, '')
  const gradientID = `${id}-chrome-glass`

  return (
    <svg viewBox="0 0 256 256" fill="none" className={cn('shrink-0', className)} aria-hidden>
      <defs>
        <linearGradient
          id={gradientID}
          x1="34"
          y1="40"
          x2="224"
          y2="218"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0" stopColor="currentColor" stopOpacity="0.98" />
          <stop offset="0.45" stopColor="currentColor" stopOpacity="0.8" />
          <stop offset="1" stopColor="currentColor" stopOpacity="0.52" />
        </linearGradient>
      </defs>
      <path d={d} fill={`url(#${gradientID})`} />
      <path
        d={d}
        fill="none"
        stroke="currentColor"
        strokeOpacity="0.3"
        strokeWidth="8"
        strokeLinejoin="round"
      />
    </svg>
  )
}

// icons.ts:27 — project rows, collapsed
export function FolderIcon({ className }: IconProps) {
  return (
    <GlassPhosphor
      className={className}
      d="M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z"
    />
  )
}

// project rows, expanded
export function FolderOpenIcon({ className }: IconProps) {
  return (
    <GlassPhosphor
      className={className}
      d="M245,110.64A16,16,0,0,0,232,104H216V88a16,16,0,0,0-16-16H130.67L102.94,51.2a16.14,16.14,0,0,0-9.6-3.2H40A16,16,0,0,0,24,64V208h0a8,8,0,0,0,8,8H211.1a8,8,0,0,0,7.59-5.47l28.49-85.47A16.05,16.05,0,0,0,245,110.64ZM93.34,64,123.2,86.4A8,8,0,0,0,128,88h72v16H69.77a16,16,0,0,0-15.18,10.94L40,158.7V64Z"
    />
  )
}

// icons.ts:22 — footer gear
export function SettingsIcon({ className }: IconProps) {
  return (
    <GlassPhosphor
      className={className}
      d="M237.94,107.21a8,8,0,0,0-3.89-5.4l-29.83-17-.12-33.62a8,8,0,0,0-2.83-6.08,111.91,111.91,0,0,0-36.72-20.67,8,8,0,0,0-6.46.59L128,41.85,97.88,25a8,8,0,0,0-6.47-.6A111.92,111.92,0,0,0,54.73,45.15a8,8,0,0,0-2.83,6.07l-.15,33.65-29.83,17a8,8,0,0,0-3.89,5.4,106.47,106.47,0,0,0,0,41.56,8,8,0,0,0,3.89,5.4l29.83,17,.12,33.63a8,8,0,0,0,2.83,6.08,111.91,111.91,0,0,0,36.72,20.67,8,8,0,0,0,6.46-.59L128,214.15,158.12,231a7.91,7.91,0,0,0,3.9,1,8.09,8.09,0,0,0,2.57-.42,112.1,112.1,0,0,0,36.68-20.73,8,8,0,0,0,2.83-6.07l.15-33.65,29.83-17a8,8,0,0,0,3.89-5.4A106.47,106.47,0,0,0,237.94,107.21ZM128,168a40,40,0,1,1,40-40A40,40,0,0,1,128,168Z"
    />
  )
}

// footer add-project
export function AddProjectIcon({ className }: IconProps) {
  return (
    <Phosphor
      className={className}
      d="M228,128a12,12,0,0,1-12,12H140v76a12,12,0,0,1-24,0V140H40a12,12,0,0,1,0-24h76V40a12,12,0,0,1,24,0v76h76A12,12,0,0,1,228,128Z"
    />
  )
}

// icons.ts:21 — footer add
export function PlusIcon({ className }: IconProps) {
  return (
    <Phosphor
      className={className}
      d="M222,128a6,6,0,0,1-6,6H134v82a6,6,0,0,1-12,0V134H40a6,6,0,0,1,0-12h82V40a6,6,0,0,1,12,0v82h82A6,6,0,0,1,222,128Z"
    />
  )
}

// icons.ts:23 — footer collapse-all
export function CollapseAllIcon({ className }: IconProps) {
  return (
    <Phosphor
      className={className}
      d="M222,128a6,6,0,0,1-6,6H40a6,6,0,0,1,0-12H216A6,6,0,0,1,222,128Zm-98.24-27.76a6,6,0,0,0,8.48,0l32-32a6,6,0,0,0-8.48-8.48L134,81.51V16a6,6,0,0,0-12,0V81.51L100.24,59.76a6,6,0,0,0-8.48,8.48Zm8.48,55.52a6,6,0,0,0-8.48,0l-32,32a6,6,0,0,0,8.48,8.48L122,174.49V240a6,6,0,0,0,12,0V174.49l21.76,21.75a6,6,0,0,0,8.48-8.48Z"
    />
  )
}

// titlebar sidebar toggle.
export function SidebarToggleIcon({ className }: IconProps) {
  return (
    <GlassPhosphor
      className={className}
      d="M216,40H40A16,16,0,0,0,24,56V200a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V56A16,16,0,0,0,216,40Zm0,160H88V56H216V200Z"
    />
  )
}

/* ---- per-tool brand marks (monochrome), shared with native ToolIcons.swift --- */

export function ClaudeMark({ className }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={cn('shrink-0', className)} aria-hidden>
      <path
        fillRule="nonzero"
        d="M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z"
      />
    </svg>
  )
}

export function CodexMark({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      fillRule="evenodd"
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <path d="M9.205 8.658v-2.26c0-.19.072-.333.238-.428l4.543-2.616c.619-.357 1.356-.523 2.117-.523 2.854 0 4.662 2.212 4.662 4.566 0 .167 0 .357-.024.547l-4.71-2.759a.797.797 0 00-.856 0l-5.97 3.473zm10.609 8.8V12.06c0-.333-.143-.57-.429-.737l-5.97-3.473 1.95-1.118a.433.433 0 01.476 0l4.543 2.617c1.309.76 2.189 2.378 2.189 3.948 0 1.808-1.07 3.473-2.76 4.163zM7.802 12.703l-1.95-1.142c-.167-.095-.239-.238-.239-.428V5.899c0-2.545 1.95-4.472 4.591-4.472 1 0 1.927.333 2.712.928L8.23 5.067c-.285.166-.428.404-.428.737v6.898zM12 15.128l-2.795-1.57v-3.33L12 8.658l2.795 1.57v3.33L12 15.128zm1.796 7.23c-1 0-1.927-.332-2.712-.927l4.686-2.712c.285-.166.428-.404.428-.737v-6.898l1.974 1.142c.167.095.238.238.238.428v5.233c0 2.545-1.974 4.472-4.614 4.472zm-5.637-5.303l-4.544-2.617c-1.308-.761-2.188-2.378-2.188-3.948A4.482 4.482 0 014.21 6.327v5.423c0 .333.143.571.428.738l5.947 3.449-1.95 1.118a.432.432 0 01-.476 0zm-.262 3.9c-2.688 0-4.662-2.021-4.662-4.519 0-.19.024-.38.047-.57l4.686 2.71c.286.167.571.167.856 0l5.97-3.448v2.26c0 .19-.07.333-.237.428l-4.543 2.616c-.619.357-1.356.523-2.117.523zm5.899 2.83a5.947 5.947 0 005.827-4.756C22.287 18.339 24 15.84 24 13.296c0-1.665-.713-3.282-1.998-4.448.119-.5.19-.999.19-1.498 0-3.401-2.759-5.947-5.946-5.947-.642 0-1.26.095-1.88.31A5.962 5.962 0 0010.205 0a5.947 5.947 0 00-5.827 4.757C1.713 5.447 0 7.945 0 10.49c0 1.666.713 3.283 1.998 4.448-.119.5-.19 1-.19 1.499 0 3.401 2.759 5.946 5.946 5.946.642 0 1.26-.095 1.88-.309a5.96 5.96 0 004.162 1.713z" />
    </svg>
  )
}

/** Official Cline bot mark from Cline's public brand kit. */
export function ClineMark({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 466.73 487.04"
      fill="currentColor"
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <path d="M463.6,275.08l-29.26-58.75v-33.83c0-56.08-45.01-101.5-100.53-101.5h-50.01c3.62-7.43,5.61-15.79,5.61-24.61,0-31.17-25.08-56.39-56.07-56.39s-56.07,25.22-56.07,56.39c0,8.82,1.99,17.17,5.61,24.61h-50.01c-55.51,0-100.52,45.42-100.52,101.5v33.83l-29.87,58.59c-3.01,5.9-3.01,12.92,0,18.81l29.87,57.93v33.83c0,56.08,45.01,101.5,100.52,101.5h200.95c55.51,0,100.53-45.42,100.53-101.5v-33.83l29.21-58.13c2.9-5.79,2.9-12.61.05-18.46ZM202.75,322.96c0,25.48-20.54,46.14-45.88,46.14s-45.88-20.66-45.88-46.14v-82.02c0-25.48,20.54-46.14,45.88-46.14s45.88,20.66,45.88,46.14v82.02ZM350.58,322.96c0,25.48-20.54,46.14-45.88,46.14s-45.88-20.66-45.88-46.14v-82.02c0-25.48,20.54-46.14,45.88-46.14s45.88,20.66,45.88,46.14v82.02Z" />
    </svg>
  )
}

export function KimiMark({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      fillRule="evenodd"
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <path d="M21.846 0a1.923 1.923 0 1 1 0 3.846H20.15a.226.226 0 0 1-.227-.226V1.923C19.923.861 20.784 0 21.846 0Z" />
      <path d="m11.065 11.199 7.257-7.2c.137-.136.06-.41-.116-.41H14.3a.164.164 0 0 0-.117.051l-7.82 7.756c-.122.12-.302.013-.302-.179V3.82c0-.127-.083-.23-.185-.23H3.186C3.083 3.59 3 3.693 3 3.82V19.77c0 .128.083.23.186.23h2.69c.103 0 .186-.102.186-.23v-3.25c0-.069.025-.135.069-.178l2.424-2.406a.158.158 0 0 1 .205-.023l6.484 4.772a7.677 7.677 0 0 0 3.453 1.283c.108.012.2-.095.2-.23v-3.06c0-.117-.07-.212-.164-.227a5.028 5.028 0 0 1-2.027-.807l-5.613-4.064c-.117-.078-.132-.279-.028-.381Z" />
    </svg>
  )
}

export function KiroMark({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      fillRule="evenodd"
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <path d="M4.594 6.677C6.67-2.226 18.746-2.211 21.16 6.632c.353 1.297 1.725 7.582-1.673 13.747-1.545 2.797-5.841 5.49-6.99 1.883C8.6 25.477 3.315 24.1 5.789 18.609l-.318.143c-3.57 1.305-3.863-1.208-3.173-2.513.45-.84.727-1.335.937-1.897.353-.975.458-1.568.593-2.498.27-1.837.277-3.607.765-5.167zm8.37.01a.92.92 0 0 0-.81.428c-.217.323-.33.825-.33 1.462 0 .705.15 1.89 1.14 1.89h.008c.757 0 1.214-.705 1.214-1.89 0-.622-.127-1.125-.367-1.455a1.014 1.014 0 0 0-.855-.435zm4.08 0a.92.92 0 0 0-.81.428c-.217.323-.33.825-.33 1.462 0 .705.15 1.89 1.14 1.89h.008c.757 0 1.215-.705 1.215-1.89 0-.622-.128-1.125-.368-1.455a1.014 1.014 0 0 0-.855-.435z" />
    </svg>
  )
}

export function GeminiMark({ className }: IconProps) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" className={cn('shrink-0', className)} aria-hidden>
      <path d="M16 8.016A8.522 8.522 0 008.016 16h-.032A8.521 8.521 0 000 8.016v-.032A8.521 8.521 0 007.984 0h.032A8.522 8.522 0 0016 7.984v.032z" />
    </svg>
  )
}

export function PiMark({ className }: IconProps) {
  return (
    <svg viewBox="0 0 800 800" fill="currentColor" className={cn('shrink-0', className)} aria-hidden>
      <path
        fillRule="evenodd"
        d="M165.29 165.29H517.36V400H400V517.36H282.65V634.72H165.29ZM282.65 282.65V400H400V282.65Z"
      />
      <path d="M517.36 400H634.72V634.72H517.36Z" />
    </svg>
  )
}

export function OpenCodeMark({ className }: IconProps) {
  return (
    <svg viewBox="0 0 240 300" fill="none" className={cn('shrink-0', className)} aria-hidden>
      <g clipPath="url(#oc-clip)">
        <mask
          id="oc-mask"
          style={{ maskType: 'luminance' }}
          maskUnits="userSpaceOnUse"
          x="0"
          y="0"
          width="240"
          height="300"
        >
          <path d="M240 0H0V300H240V0Z" fill="white" />
        </mask>
        <g mask="url(#oc-mask)">
          <path d="M180 240H60V120H180V240Z" fill="currentColor" fillOpacity="0.5" />
          <path d="M180 60H60V240H180V60ZM240 300H0V0H240V300Z" fill="currentColor" />
        </g>
      </g>
      <defs>
        <clipPath id="oc-clip">
          <rect width="240" height="300" fill="white" />
        </clipPath>
      </defs>
    </svg>
  )
}

export function CursorMark({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      fillRule="evenodd"
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <path d="M22.106 5.68L12.5.135a.998.998 0 00-.998 0L1.893 5.68a.84.84 0 00-.419.726v11.186c0 .3.16.577.42.727l9.607 5.547a.999.999 0 00.998 0l9.608-5.547a.84.84 0 00.42-.727V6.407a.84.84 0 00-.42-.726zm-.603 1.176L12.228 22.92c-.063.108-.228.064-.228-.061V12.34a.59.59 0 00-.295-.51l-9.11-5.26c-.107-.062-.063-.228.062-.228h18.55c.264 0 .428.286.296.514z" />
    </svg>
  )
}

export function GrokMark({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      fillRule="evenodd"
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <path d="M9.27 15.29l7.978-5.897c.391-.29.95-.177 1.137.272.98 2.369.542 5.215-1.41 7.169-1.951 1.954-4.667 2.382-7.149 1.406l-2.711 1.257c3.889 2.661 8.611 2.003 11.562-.953 2.341-2.344 3.066-5.539 2.388-8.42l.006.007c-.983-4.232.242-5.924 2.75-9.383.06-.082.12-.164.179-.248l-3.301 3.305v-.01L9.267 15.292M7.623 16.723c-2.792-2.67-2.31-6.801.071-9.184 1.761-1.763 4.647-2.483 7.166-1.425l2.705-1.25a7.808 7.808 0 00-1.829-1A8.975 8.975 0 005.984 5.83c-2.533 2.536-3.33 6.436-1.962 9.764 1.022 2.487-.653 4.246-2.34 6.022-.599.63-1.199 1.259-1.682 1.925l7.62-6.815" />
    </svg>
  )
}

export function MuseMark({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 800 800"
      fill="currentColor"
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <path d="M197 313C131 313 63 277 63 225C63 177 120 145 182 145C248 145 316 181 316 234C316 284 256 313 197 313Z" />
      <path d="M95 567C55 567 29 540 29 499C29 429 105 349 175 349C216 349 240 376 240 416C240 488 164 567 95 567Z" />
      <path d="M365 660C365 712 338 769 290 769C234 769 194 691 194 626C194 573 221 518 269 518C327 518 365 599 365 660Z" />
      <path d="M553 742C482 742 398 680 398 614C398 571 434 550 476 550C548 550 633 613 633 677C633 719 596 742 553 742Z" />
      <path d="M785 439C785 505 695 555 627 555C575 555 542 525 542 486C542 422 630 371 700 371C751 371 785 398 785 439Z" />
      <path d="M596 356C550 356 525 310 525 261C525 195 570 109 632 109C678 109 702 157 702 205C702 269 659 356 596 356Z" />
      <path d="M502 172C502 213 480 243 439 243C370 243 303 157 303 87C303 47 325 15 367 15C436 15 502 101 502 172Z" />
    </svg>
  )
}

// ProjectItem.svelte:307 — Lucide "split", rotated 90° (workspace / session glyph)
export function BranchIcon({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={cn('shrink-0 rotate-90', className)}
      aria-hidden
    >
      <path d="M16 3h5v5" />
      <path d="M8 3H3v5" />
      <path d="M12 22v-8.3a4 4 0 0 0-1.172-2.872L3 3" />
      <path d="m15 9 6-6" />
    </svg>
  )
}

// Phosphor "tree-structure" (fill) — marks an Orchestrator session that can
// branch out and drive its sibling sessions via Unpeel Sessions MCP.
export function OrchestratorIcon({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 256 256"
      fill="currentColor"
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <path d="M144,96V80H128a8,8,0,0,0-8,8v80a8,8,0,0,0,8,8h16V160a16,16,0,0,1,16-16h48a16,16,0,0,1,16,16v48a16,16,0,0,1-16,16H160a16,16,0,0,1-16-16V192H128a24,24,0,0,1-24-24V136H72v8a16,16,0,0,1-16,16H24A16,16,0,0,1,8,144V112A16,16,0,0,1,24,96H56a16,16,0,0,1,16,16v8h32V88a24,24,0,0,1,24-24h16V48a16,16,0,0,1,16-16h48a16,16,0,0,1,16,16V96a16,16,0,0,1-16,16H160A16,16,0,0,1,144,96Z" />
    </svg>
  )
}

// The real app's session pin — Phosphor tilted "push-pin", copied verbatim
// from the native ChromeIcons.swift `.pin` glyph (path is already tilted).
// Shown only on pinned session rows.
export function PinIcon({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 256 256"
      fill="currentColor"
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <path d="M233.91,82.79,173.22,22.1a14,14,0,0,0-19.81,0L98.93,76.77c-9.52-3.25-34-8.34-59.71,12.41A14,14,0,0,0,38.1,110l49.71,49.71-44.05,44a6,6,0,1,0,8.48,8.48l44.05-44.05L146,217.89a14,14,0,0,0,9.9,4.11q.49,0,1,0a14,14,0,0,0,10.19-5.54c19.72-26.21,17.15-47.23,12.46-59.3l54.37-54.55A14,14,0,0,0,233.91,82.79ZM225.42,94.1h0l-57.27,57.46a6,6,0,0,0-1.11,6.92c9.94,19.88-1.71,40.32-9.54,50.72a2,2,0,0,1-3,.2L46.58,101.51a2,2,0,0,1,.18-3c12.5-10.09,24.5-12.76,33.7-12.76a42.13,42.13,0,0,1,17.25,3.41A6,6,0,0,0,104.64,88L161.9,30.59a2,2,0,0,1,2.83,0l60.69,60.68A2,2,0,0,1,225.42,94.1Z" />
    </svg>
  )
}

/** GitHub octicon mark (site-only — not part of the shared app chrome set). */
export function GitHubMark({ className }: IconProps) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" className={cn('shrink-0', className)} aria-hidden>
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.012 8.012 0 0 0 16 8c0-4.42-3.58-8-8-8z" />
    </svg>
  )
}
