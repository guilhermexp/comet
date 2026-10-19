/**
 * scripts/ci/release-profile-gate.test.cjs
 *
 * Tests the public decision of evaluateGate offline with `node --test`.
 */

const { describe, it } = require('node:test');
const assert = require('node:assert/strict');
const { evaluateGate } = require('./release-profile-gate.cjs');

function mockGithub({ pages = [], jobs = {} }) {
  return {
    rest: {
      actions: {
        listWorkflowRuns: async ({ page = 1 }) => ({
          data: { workflow_runs: pages[page - 1] || [] },
        }),
        listJobsForWorkflowRun: async ({ run_id }) => ({
          data: { jobs: jobs[run_id] || [] },
        }),
      },
    },
  };
}

describe('evaluateGate public decisions', () => {
  const now = new Date('2026-10-19T12:00:00Z');
  const sha = 'sha-current';
  const ctx = {
    eventName: 'schedule',
    repo: { owner: 'org', repo: 'repo' },
    sha,
    ref: 'refs/heads/main',
    runId: 9999,
  };

  const iso = (daysAgo) => new Date(now.getTime() - daysAgo * 86400000).toISOString();

  const greenJobs = (daysAgo) => [
    { name: 'core-tests', status: 'completed', conclusion: 'success', completed_at: iso(daysAgo) },
    { name: 'ui-tests', status: 'completed', conclusion: 'success', completed_at: iso(daysAgo) },
  ];

  const skippedJobs = () => [
    { name: 'gate', status: 'completed', conclusion: 'success' },
    { name: 'core-tests', status: 'completed', conclusion: 'skipped' },
    { name: 'ui-tests', status: 'completed', conclusion: 'skipped' },
  ];

  const testCases = [
    {
      name: 'manual trigger always runs',
      context: { ...ctx, eventName: 'workflow_dispatch' },
      github: mockGithub({}),
      expected: true,
    },
    {
      name: 'first run with empty history runs',
      context: ctx,
      github: mockGithub({ pages: [[]] }),
      expected: true,
    },
    {
      name: 'SHA changed runs',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 1, head_sha: 'sha-old', head_branch: 'main', conclusion: 'success', created_at: iso(2) }]],
        jobs: { 1: greenJobs(2) },
      }),
      expected: true,
    },
    {
      name: 'same SHA with recent green run (< 7 days) skips',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 2, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(2) }]],
        jobs: { 2: greenJobs(2) },
      }),
      expected: false,
    },
    {
      name: 'same SHA with weekly green run (>= 7 days) runs',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 3, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(7.5) }]],
        jobs: { 3: greenJobs(7.5) },
      }),
      expected: true,
    },
    {
      name: 'recent failure on same SHA does not mask failure: older green + newer red runs',
      context: ctx,
      github: mockGithub({
        pages: [[
          { id: 5, head_sha: sha, head_branch: 'main', conclusion: 'failure', created_at: iso(1) },
          { id: 4, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(3) },
        ]],
        jobs: {
          5: [{ name: 'core-tests', status: 'completed', conclusion: 'failure' }],
          4: greenJobs(3),
        },
      }),
      expected: true,
    },
    {
      name: 'recent cancellation on same SHA does not mask failure: older green + newer cancelled runs',
      context: ctx,
      github: mockGithub({
        pages: [[
          { id: 7, head_sha: sha, head_branch: 'main', conclusion: 'cancelled', created_at: iso(1) },
          { id: 6, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(3) },
        ]],
        jobs: {
          7: [{ name: 'core-tests', status: 'completed', conclusion: 'cancelled' }],
          6: greenJobs(3),
        },
      }),
      expected: true,
    },
    {
      name: 'run cancelled overall cannot serve as green baseline even if jobs succeeded',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 8, head_sha: sha, head_branch: 'main', conclusion: 'cancelled', created_at: iso(2) }]],
        jobs: { 8: greenJobs(2) },
      }),
      expected: true,
    },
    {
      name: 'gate-only green does not renew timer: 8d old green + 1d old skipped runs',
      context: ctx,
      github: mockGithub({
        pages: [[
          { id: 10, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(1) },
          { id: 9, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(8) },
        ]],
        jobs: {
          10: skippedJobs(),
          9: greenJobs(8),
        },
      }),
      expected: true,
    },
    {
      name: 'gate-only green preserves recent green: 3d old green + 1d old skipped skips',
      context: ctx,
      github: mockGithub({
        pages: [[
          { id: 12, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(1) },
          { id: 11, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(3) },
        ]],
        jobs: {
          12: skippedJobs(),
          11: greenJobs(3),
        },
      }),
      expected: false,
    },
    {
      name: 'runs on different branch are ignored',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 13, head_sha: sha, head_branch: 'feature', conclusion: 'success', created_at: iso(2) }]],
        jobs: { 13: greenJobs(2) },
      }),
      expected: true,
    },
    {
      name: 'non-branch ref defaults to run',
      context: { ...ctx, ref: 'refs/tags/v1.0.0' },
      github: mockGithub({}),
      expected: true,
    },
    {
      name: 'missing timestamp on green run defaults to run without inventing dates',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 14, head_sha: sha, head_branch: 'main', conclusion: 'success' }]],
        jobs: { 14: [{ name: 'core-tests', status: 'completed', conclusion: 'success' }, { name: 'ui-tests', status: 'completed', conclusion: 'success' }] },
      }),
      expected: true,
    },
    {
      name: 'prefix job name (core-tests-other) rejected, exact equality required',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 15, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(2) }]],
        jobs: {
          15: [
            { name: 'core-tests-other', status: 'completed', conclusion: 'success', completed_at: iso(2) },
            { name: 'ui-tests', status: 'completed', conclusion: 'success', completed_at: iso(2) },
          ],
        },
      }),
      expected: true,
    },
    {
      name: 'paginated history with empty/skipped runs stops at temporal cutoff and runs',
      context: ctx,
      github: mockGithub({
        pages: [
          [{ id: 16, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(2) }],
          [{ id: 17, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(10) }],
        ],
        jobs: {
          16: skippedJobs(),
          17: greenJobs(10),
        },
      }),
      expected: true,
    },
    {
      name: 'paginated history finds recent green on page 2 and skips',
      context: ctx,
      github: mockGithub({
        pages: [
          [{ id: 18, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(1) }],
          [{ id: 19, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(3) }],
        ],
        jobs: {
          18: skippedJobs(),
          19: greenJobs(3),
        },
      }),
      expected: false,
    },
    {
      name: 'API failure in listWorkflowRuns fails-open',
      context: ctx,
      github: {
        rest: {
          actions: {
            listWorkflowRuns: async () => { throw new Error('API 500'); },
          },
        },
      },
      expected: true,
    },
    {
      name: 'API failure in listJobsForWorkflowRun fails-open',
      context: ctx,
      github: {
        rest: {
          actions: {
            listWorkflowRuns: async () => ({
              data: { workflow_runs: [{ id: 20, head_sha: sha, head_branch: 'main', conclusion: 'success', created_at: iso(1) }] },
            }),
            listJobsForWorkflowRun: async () => { throw new Error('API 403'); },
          },
        },
      },
      expected: true,
    },
    {
      name: 'missing context fails-open',
      context: {},
      github: mockGithub({}),
      expected: true,
    },
    {
      name: 'exact seven day boundary runs',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 21, head_sha: sha, head_branch: 'main', conclusion: 'success' }]],
        jobs: { 21: greenJobs(7) },
      }),
      expected: true,
    },
    {
      name: 'future completion timestamp cannot suppress tests',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 22, head_sha: sha, head_branch: 'main', conclusion: 'success' }]],
        jobs: { 22: greenJobs(-1) },
      }),
      expected: true,
    },
    {
      name: 'missing branch on prior run cannot establish a baseline',
      context: ctx,
      github: mockGithub({
        pages: [[{ id: 23, head_sha: sha, conclusion: 'success' }]],
        jobs: { 23: greenJobs(1) },
      }),
      expected: true,
    },
  ];

  for (const tc of testCases) {
    it(tc.name, async () => {
      const res = await evaluateGate({
        github: tc.github,
        context: tc.context,
        now,
      });
      assert.equal(res.should_run, tc.expected);
    });
  }
});
