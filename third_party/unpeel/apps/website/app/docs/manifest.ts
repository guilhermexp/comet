/**
 * Docs manifest: the single source of truth for doc slugs, titles, and
 * sidebar grouping. The server route validates slugs against it and the Docs
 * page builds the sidebar from it. Content lives as markdown files in this
 * directory, one per slug (`<slug>.md`), loaded by the Docs page via
 * import.meta.glob.
 */

export type DocMeta = {
  slug: string
  title: string
  /** One-liner under the page title. */
  description: string
}

export type DocGroup = { label: string; items: DocMeta[] }

/**
 * Feature gate for Unpeel Link/Remote (the mobile/relay feature). The iOS app
 * ships behind its own native flag; keep the public doc hidden until it's
 * approved and released. Build with `VITE_UNPEEL_REMOTE=1` to publish it.
 * Vite inlines `import.meta.env.VITE_*` at build time, so an unset flag
 * folds this away in both the client and worker bundles.
 */
const UNPEEL_REMOTE_ENABLED = import.meta.env.VITE_UNPEEL_REMOTE === '1'

/**
 * Feature gate for Workspaces (multiple isolated app instances). The native
 * feature has shipped; this build flag only controls whether its public doc
 * appears. `VITE_UNPEEL_PROFILES` remains accepted for existing deploy config.
 */
const UNPEEL_WORKSPACES_ENABLED =
  import.meta.env.VITE_UNPEEL_WORKSPACES === '1' ||
  import.meta.env.VITE_UNPEEL_PROFILES === '1'

/**
 * Feature gate for the terminal UI / CLI (`unpeel`). The binary isn't in a
 * released build yet; publish the doc with `VITE_UNPEEL_TUI=1` once it ships.
 */
const UNPEEL_TUI_ENABLED = import.meta.env.VITE_UNPEEL_TUI === '1'

const TERMINAL_UI_DOC: DocMeta = {
  slug: 'terminal-ui',
  title: 'Terminal UI & CLI',
  description:
    'Run and steer the same fleet from a terminal — on your Mac, over SSH, or on a box with no desktop at all.'
}

const WORKSPACES_DOC: DocMeta = {
  slug: 'workspaces',
  title: 'Workspaces',
  description:
    'Run several fully isolated Unpeels on one Mac — separate sessions, settings, and phone pairing.'
}

/**
 * Feature gate for Unpeel Apps (standalone terminal apps rendered by every
 * frontend). Published ahead of the first apps shipping (operator decision
 * 2026-08-12) — the flag remains so the group can be pulled from a build if
 * needed. Also gates the /apps/skill.md route in server.ts.
 */
const UNPEEL_APPS_ENABLED = import.meta.env.VITE_UNPEEL_APPS === '1'

const UNPEEL_APPS_GROUP: DocGroup = {
  label: 'Unpeel Apps',
  items: [
    {
      slug: 'unpeel-apps',
      title: 'Overview',
      description:
        'Standalone terminal apps — tasks, notes, dashboards — that light up inside Unpeel: sidebar status, panels, widgets, and your phone.'
    },
    {
      slug: 'building-apps',
      title: 'Building an app',
      description:
        'Any language that can draw in a terminal can be an Unpeel App; Rust with the unpeel-ui crate is the golden path.'
    },
    {
      slug: 'unpeel-ui',
      title: 'The unpeel-ui SDK',
      description:
        'The Rust SDK Apps are primarily built with: shared style, status, and keybindings, plus one view definition that renders in any terminal and serializes for native renderers.'
    }
  ]
}

const REMOTE_ACCESS_GROUP: DocGroup = {
  label: 'Remote access',
  items: [
    {
      slug: 'remote',
      title: 'Remote access',
      description:
        'Reach your Hosts from a phone or any terminal today; the native Mac Host picker is a development preview.'
    },
    {
      slug: 'unpeel-link',
      title: 'Unpeel Link',
      description:
        'The operated, end-to-end encrypted relay and push service for when you are away — with no cloud content store.'
    },
    {
      slug: 'session-gallery',
      title: 'Session gallery',
      description:
        'Every image tied to a session on your phone — browser screenshots, plus your own uploads and markup.'
    }
  ]
}

/** The `unpeel` binary: the terminal UI and the scriptable one-shot CLI.
 *  Gated together with the TUI doc until the binary is in a released build. */
const UNPEEL_CLI_GROUP: DocGroup = {
  label: 'Unpeel CLI',
  items: [
    TERMINAL_UI_DOC,
    {
      slug: 'cli',
      title: 'CLI reference',
      description:
        'Launch, drive, and script sessions from any shell — ls, new, send, wait, logs, restart, and friends.'
    }
  ]
}

export const DOC_GROUPS: DocGroup[] = [
  {
    label: 'Start here',
    items: [
      {
        slug: 'getting-started',
        title: 'Getting started',
        description: 'Install Unpeel and launch your first agent session.'
      },
      {
        slug: 'agents',
        title: 'Supported agents',
        description: 'Every CLI agent Unpeel integrates with, and what each integration gives you.'
      }
    ]
  },
  {
    label: 'Core concepts',
    items: [
      {
        slug: 'sessions',
        title: 'Sessions & terminals',
        description: 'How sessions survive app restarts, and how restart-with-resume works.'
      },
      {
        slug: 'projects-and-presets',
        title: 'Projects & presets',
        description: 'Organize sessions by project and launch every agent exactly how you like it.'
      },
      {
        slug: 'agent-runtimes',
        title: 'Agent runtimes',
        description:
          'How managed launches differ from agents observed later in a terminal, and which integrations safely follow each one.'
      },
      {
        slug: 'worktrees',
        title: 'Git worktrees',
        description: 'Run multiple agents on the same repository without them touching each other.'
      },
      {
        slug: 'activity',
        title: 'Activity & the menu bar',
        description: 'Busy, done, needs-you: how Unpeel tracks agents and pings you from the menu bar.'
      },
      // Keep the public page optional even though Workspaces ships natively.
      ...(UNPEEL_WORKSPACES_ENABLED ? [WORKSPACES_DOC] : [])
    ]
  },
  // Remote access right after the core concepts (mobile-gated as before).
  ...(UNPEEL_REMOTE_ENABLED ? [REMOTE_ACCESS_GROUP] : []),
  // The unpeel binary: terminal UI + scriptable CLI (gated until released).
  ...(UNPEEL_TUI_ENABLED ? [UNPEEL_CLI_GROUP] : []),
  {
    label: 'Unpeel MCP',
    items: [
      {
        slug: 'unpeel-mcp',
        title: 'Overview',
        description: 'One built-in MCP server, one tool per capability — sessions and isolated browser access in production 0.2.'
      },
      {
        slug: 'sessions-mcp',
        title: 'Sessions use',
        description: 'Let agents read your fleet, coordinate freely inside sidebar groups, and ask before writing across groups.'
      },
      {
        slug: 'browser-mcp',
        title: 'Browser use',
        description: 'Give any agent a real, isolated browser powered by agent-browser.'
      },
      {
        slug: 'computer-mcp',
        title: 'Computer use',
        description: 'Development-only Mac control, withheld from production while its permission boundary is hardened.'
      }
    ]
  },
  {
    label: 'Contribute',
    items: [
      {
        slug: 'adding-agent-runtime',
        title: 'Add an agent runtime',
        description:
          'Contribute recognition, setup, lifecycle, resume, MCP, and transcripts as one built-in runtime package.'
      }
    ]
  },
  // Unpeel Apps is WIP — hidden from the sidebar (and unroutable) for now
  // regardless of VITE_UNPEEL_APPS; uncomment to publish.
  // ...(UNPEEL_APPS_ENABLED ? [UNPEEL_APPS_GROUP] : []),
  // Link, licensing & updates is hidden while the Link/licensing story is
  // reworked; uncomment to publish again.
  // {
  //   label: 'Link, licensing & updates',
  //   items: [
  //     {
  //       slug: 'licensing-and-updates',
  //       title: 'The free app & Unpeel Link',
  //       description: 'The free local app, paid operated Link service, legacy Pro activation, and updates.'
  //     }
  //   ]
  // },
  {
    label: 'Troubleshooting',
    items: [
      {
        slug: 'troubleshooting',
        title: 'List & stop everything',
        description:
          'Inspect or stop Unpeel’s background processes from the terminal — even with the app closed.'
      }
    ]
  }
]

export const DOCS: DocMeta[] = DOC_GROUPS.flatMap((g) => g.items)

export const DEFAULT_DOC = 'getting-started'

export function isDocSlug(slug: string): boolean {
  return DOCS.some((d) => d.slug === slug)
}
