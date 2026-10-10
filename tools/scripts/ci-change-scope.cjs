// Only explicitly covered non-Rust surfaces may skip Rust compilation.
// Dependency, build, workflow, generated and unknown changes run the full gates.
function selectChecks(files) {
  if (!Array.isArray(files) || !files.length) return { rust: true, installer: true };
  const paths = files.flatMap(f => [f.filename, f.previous_filename].filter(Boolean));
  if (files.some(f => typeof f.filename !== 'string' || !f.filename)) return { rust: true, installer: true };
  const docs = p => (p.startsWith('docs/') || p.endsWith('.md')) && !p.startsWith('docs/genome/');
  const windows = p => /^(install\.ps1|tools\/scripts\/(lib\/(windows-(service|prepared|engine-receipt|elevation|media-reconciliation)|install-common|win-modules)\.ps1|tests\/windows-(service|process|airc-download|xz)\.test\.ps1|sync-windows-bootstrap\.ps1|register-core-service\.ps1|run-service-hidden\.ps1))$/.test(p);
  const rust = paths.some(p => !docs(p) && !windows(p));
  return { rust, installer: rust || paths.some(windows) };
}
module.exports = { selectChecks };
