// Hourly publication admission. No build, ledger, tag, or runtime mutation here.
import { pathToFileURL } from 'node:url';

export const platforms = ['macos-arm64', 'macos-x86_64', 'windows-x86_64'];

export async function planPublication(api, repo) {
  const prefix = `/repos/${repo}`;
  const { sha } = await api(`${prefix}/commits/canary`);
  if (!/^[0-9a-f]{40}$/.test(sha)) throw new Error('canary did not resolve to a full SHA');
  const tag = `canary-${sha.slice(0, 12)}`;
  const release = await api(`${prefix}/releases/tags/${tag}`, { allow404: true });
  if (release !== null && !Array.isArray(release.assets)) throw new Error('release assets response is malformed');
  let complete = release !== null && !release.draft && Array.isArray(release.assets);
  if (complete) {
    for (const platform of platforms) {
      const name = `continuum-core-${platform}`;
      const assets = ['.tar.gz', '.tar.gz.sha256', '.json'].map(suffix =>
        release.assets.find(asset => asset.name === name + suffix && asset.size > 0));
      if (assets.some(asset => !asset)) { complete = false; break; }
      // Manifest is the publication commit point, uploaded last by the existing publisher.
      if (!Number.isSafeInteger(assets[2].id)) throw new Error('manifest asset id is malformed');
      const manifest = await api(`${prefix}/releases/assets/${assets[2].id}`, { asset: true });
      if (manifest.git_sha !== sha || manifest.platform !== platform ||
          manifest.archive !== `${name}.tar.gz` || !/^[0-9a-f]{64}$/.test(manifest.sha256)) {
        complete = false;
        break;
      }
    }
  }
  if (complete) return { sha, action: 'skip', reason: 'all platform manifests published' };
  const runs = await api(`${prefix}/actions/workflows/core-binaries.yml/runs?head_sha=${sha}&per_page=100`);
  if (!Array.isArray(runs.workflow_runs)) throw new Error('workflow runs response is malformed');
  if (runs.workflow_runs.some(run => run.head_sha === sha && run.event !== 'pull_request' && run.status !== 'completed')) {
    return { sha, action: 'skip', reason: 'publication already queued or running' };
  }
  return { sha, action: 'dispatch', reason: 'latest canary is not fully published' };
}

export async function runPublication(api, repo, dryRun) {
  const plan = await planPublication(api, repo);
  if (plan.action === 'dispatch' && !dryRun) {
    // GitHub resolves canary when accepting dispatch. A merge racing this read may
    // advance it; do not misreport the inspected SHA as a pinned build revision.
    await api(`/repos/${repo}/actions/workflows/core-binaries.yml/dispatches`, {
      method: 'POST', body: { ref: 'canary' },
    });
  }
  return { ...plan, dryRun, dispatched: plan.action === 'dispatch' && !dryRun };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const repo = process.env.GITHUB_REPOSITORY;
  const token = process.env.GH_TOKEN;
  if (!repo || !token) throw new Error('GITHUB_REPOSITORY and GH_TOKEN are required');
  if (!['true', 'false'].includes(process.env.DRY_RUN)) throw new Error('DRY_RUN must be true or false');
  const api = async (path, { allow404 = false, asset = false, method = 'GET', body } = {}) => {
    const response = await fetch(`https://api.github.com${path}`, {
      method, headers: { Authorization: `Bearer ${token}`, Accept: asset ? 'application/octet-stream' : 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28' },
      body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(30000),
    });
    if (allow404 && response.status === 404) return null;
    if (!response.ok) throw new Error(`GitHub ${method} ${path}: HTTP ${response.status}`);
    return response.status === 204 ? null : response.json();
  };
  console.log(JSON.stringify(await runPublication(api, repo, process.env.DRY_RUN === 'true')));
}
