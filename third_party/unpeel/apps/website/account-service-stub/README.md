# @unpeel/account-service (public stub)

The website worker (`apps/website`) mounts Unpeel's account/licensing/Link
service, whose implementation is closed source (see
`docs/plans/open-source.md`). This directory is the **public stand-in**: the
same export surface with no service behavior, so the open repo installs,
typechecks, and builds out of the box.

- Service API routes answer `501` with an honest explanation.
- Service pages redirect to `/`.
- Middleware and background handlers are no-ops.
- Everything else on the site (pages, docs, downloads) works normally.

**Selection is automatic.** `vite.config.ts` and `tsconfig.json` check for the
private sibling checkout (`../../../unpeel-account`); when it exists, the
module id `@unpeel/account-service` resolves to the real source and this stub
is ignored. `UNPEEL_FORCE_ACCOUNT_STUB=1` forces the stub even with the
sibling present (useful for testing the open-source build on a machine that
has the private repo).

If you change the real package's export surface, update `src/index.ts` here
in the same change — the open repo's `bun run check` typechecks against this
stub, and a drifted stub breaks exactly the builds you can't see. After
editing the stub, run `bun install --force` in apps/website: bun snapshots
`file:` dependencies into its store, so builds otherwise keep using the old
copy.

The stub deliberately does not mirror the service's operator tooling
(admin): those paths aren't registered at all, so on open-source builds they
404 like any unknown URL, and the words don't appear in the open repo.
