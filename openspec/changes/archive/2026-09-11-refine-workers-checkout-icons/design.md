## Decisions
Reuse existing project parent relationships and current-branch resolver. Worktree rows replace their leading folder/chevron with WORKER_BRANCH and show the branch as their label. Root projects keep their names and folder glyphs with no Local/main subtitle. PR icons retain existing click/tooltip behavior.

## Validation
Existing tree, selection and branch tests cover unchanged derivations. This correction is paint/layout only; native demo is the visual seam, with no new implementation-mirroring unit tests. Run workspace tests and build.
