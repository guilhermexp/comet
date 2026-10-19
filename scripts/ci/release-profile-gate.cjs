/**
 * scripts/ci/release-profile-gate.cjs
 *
 * Gate helper for .github/workflows/release-profile-tests.yml.
 * Avoids executing expensive daily release-profile suites when the codebase has not changed
 * and has passed recently, while guaranteeing a full verification at least once every 7 days.
 */

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;
const REQUIRED_JOBS = ['core-tests', 'ui-tests'];
const WORKFLOW_ID = 'release-profile-tests.yml';
const MAX_PAGES = 5;
const PER_PAGE = 30;

function extractCompletedAt(matchedJobs, run) {
  const timestamps = [];
  for (const job of matchedJobs) {
    const raw = job?.completed_at || job?.updated_at;
    if (raw) {
      const d = new Date(raw);
      if (!isNaN(d.getTime())) timestamps.push(d.getTime());
    }
  }
  if (timestamps.length === matchedJobs.length) {
    return new Date(Math.max(...timestamps));
  }
  const fallback = run?.updated_at || run?.run_started_at;
  if (fallback) {
    const d = new Date(fallback);
    if (!isNaN(d.getTime())) return d;
  }
  return null;
}

function isGateOnlySkipped(run, jobs) {
  if (run?.conclusion !== 'success') return false;
  const testJobs = (jobs || []).filter(j => REQUIRED_JOBS.includes(j.name));
  if (testJobs.length === 0) {
    const gateJob = (jobs || []).find(j => j.name === 'gate');
    return Boolean(gateJob && gateJob.conclusion === 'success');
  }
  return testJobs.every(j => j.conclusion === 'skipped');
}

function getSuccessfulRequiredJobs(jobs) {
  const matched = [];
  for (const name of REQUIRED_JOBS) {
    const job = (jobs || []).find(j => j.name === name);
    if (!job || job.status !== 'completed' || job.conclusion !== 'success') {
      return null;
    }
    matched.push(job);
  }
  return matched;
}

async function evaluateGate({ github, context, core = null, now = new Date() }) {
  try {
    if (context?.eventName === 'workflow_dispatch') {
      return { should_run: true, reason: 'Manual trigger (workflow_dispatch)' };
    }

    const owner = context?.repo?.owner;
    const repo = context?.repo?.repo;
    const currentSha = context?.sha;
    const currentRunId = context?.runId;
    const ref = context?.ref || '';

    if (!owner || !repo || !currentSha || !ref.startsWith('refs/heads/')) {
      return { should_run: true, reason: 'Missing repo context, commit SHA, or valid branch ref' };
    }
    const branch = ref.slice('refs/heads/'.length);
    const cutoffDate = new Date(now.getTime() - SEVEN_DAYS_MS);

    for (let page = 1; page <= MAX_PAGES; page++) {
      let runsResp;
      try {
        runsResp = await github.rest.actions.listWorkflowRuns({
          owner,
          repo,
          workflow_id: WORKFLOW_ID,
          branch,
          status: 'completed',
          per_page: PER_PAGE,
          page,
        });
      } catch (err) {
        return { should_run: true, reason: `API error listing workflow runs: ${err.message}` };
      }

      const runs = runsResp?.data?.workflow_runs || [];
      if (runs.length === 0) break;

      for (const run of runs) {
        if (run.id === currentRunId) continue;
        if (run.head_branch !== branch) continue;

        let jobsResp;
        try {
          jobsResp = await github.rest.actions.listJobsForWorkflowRun({
            owner,
            repo,
            run_id: run.id,
            filter: 'latest',
            per_page: 100,
          });
        } catch (err) {
          return { should_run: true, reason: `API error fetching jobs for run ${run.id}: ${err.message}` };
        }

        const jobs = jobsResp?.data?.jobs || [];
        if (isGateOnlySkipped(run, jobs)) continue;

        // Any non-gate-only run represents a test suite attempt.
        // If it was cancelled or failed, we must re-test without reusing older green runs.
        if (run.conclusion !== 'success') {
          return { should_run: true, reason: `Latest test run (${run.id}) was not successful (${run.conclusion})` };
        }

        const matchedJobs = getSuccessfulRequiredJobs(jobs);
        if (!matchedJobs) {
          return { should_run: true, reason: `Latest test run (${run.id}) missing successful required jobs` };
        }

        if (run.head_sha !== currentSha) {
          return { should_run: true, reason: 'Commit SHA changed since last green run' };
        }

        const completedAt = extractCompletedAt(matchedJobs, run);
        if (!completedAt) {
          return { should_run: true, reason: 'Missing completion timestamp on last successful run' };
        }

        const ageMs = now.getTime() - completedAt.getTime();
        if (ageMs >= 0 && ageMs < SEVEN_DAYS_MS) {
          return { should_run: false, reason: 'Release profile tests recently passed on this SHA' };
        }
        return { should_run: true, reason: 'Weekly schedule refresh (>= 7 days since last run)' };
      }

      const oldest = runs[runs.length - 1];
      const oldestDate = new Date(oldest?.created_at || oldest?.updated_at);
      if (!isNaN(oldestDate.getTime()) && oldestDate < cutoffDate) {
        break;
      }
    }

    return { should_run: true, reason: 'No recent successful release-profile test run found' };
  } catch (err) {
    return { should_run: true, reason: `Unexpected gate error: ${err.message}` };
  }
}

module.exports = { evaluateGate };
