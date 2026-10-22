const { isDeepStrictEqual } = require('node:util');

const publisherPath = '.github/workflows/core-binaries.yml';
const publicationGate = "${{ !cancelled() && needs.publication-scope.outputs.publication != 'false' }}";
const schedulingPaths = new Set([
  '.github/workflows/core-binaries-hourly.yml',
  'tools/scripts/core-binaries-hourly.mjs',
  'tools/scripts/tests/core-binaries-hourly.test.mjs',
  'tools/scripts/ci-change-scope.cjs',
  'tools/scripts/tests/ci-change-scope.test.cjs',
]);
const docs = p => (p.startsWith('docs/') || p.endsWith('.md')) && !p.startsWith('docs/genome/');
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);

function publicationConfig(value) {
  if (!object(value) || !object(value.jobs) || !object(value.jobs.build) || !Array.isArray(value.jobs.build.steps)) {
    throw new Error('Missing publication build configuration');
  }
  const copy = structuredClone(value);
  delete copy.on;
  delete copy.jobs['publication-scope'];
  const build = copy.jobs.build;
  // Strip only our exact admission wiring. Unknown conditions/dependencies are protected.
  if (build.needs === 'publication-scope' && build.if === publicationGate) {
    delete build.needs;
    delete build.if;
  }
  return copy;
}

function selectPublication(paths, evidence) {
  if (evidence.eventName !== 'pull_request' || evidence.complete !== true) return true;
  if (paths.some(p => p !== publisherPath && !docs(p) && !schedulingPaths.has(p)
      && p !== 'core/continuum-cli-lifecycle/src/prebuilt_artifact/tests.rs')) return true;
  if (!paths.includes(publisherPath)) return false;
  try {
    return !isDeepStrictEqual(publicationConfig(evidence.publicationBefore), publicationConfig(evidence.publicationAfter));
  } catch {
    return true;
  }
}

// Only explicitly covered non-Rust surfaces may skip Rust compilation.
// Dependency, build, workflow, generated and unknown changes run the full gates.
function selectChecks(files, evidence = {}) {
  const full = { rust: true, installer: true, publication: true };
  if (!Array.isArray(files) || !files.length) return full;
  if (files.some(f => !f || typeof f.filename !== 'string' || !f.filename
      || (f.previous_filename !== undefined && (typeof f.previous_filename !== 'string' || !f.previous_filename)))) return full;
  const paths = files.flatMap(f => [f.filename, f.previous_filename].filter(Boolean));
  const windows = p => /^(install\.ps1|tools\/scripts\/(lib\/(windows-(service|prepared|engine-receipt|elevation|media-reconciliation)|install-common|win-modules|win-engine)\.ps1|tests\/windows-(service|process|airc-download|xz)\.test\.ps1|sync-windows-bootstrap\.ps1|register-core-service\.ps1|run-service-hidden\.ps1))$/.test(p);
  const rust = paths.some(p => !docs(p) && !windows(p));
  return { rust, installer: rust || paths.some(windows), publication: selectPublication(paths, evidence) };
}
module.exports = { selectChecks };
