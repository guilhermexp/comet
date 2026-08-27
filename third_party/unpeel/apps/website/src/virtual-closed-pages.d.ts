// Types for the virtual module emitted by vite-plugins/closed-pages.ts.
declare module 'virtual:closed-pages' {
  export const closedPages: Record<
    string,
    () => Promise<{ default: import('@inertiajs/react').ResolvedComponent }>
  >
}
