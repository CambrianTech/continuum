// Re-vendor continuum view payloads from the canonical ts-rs output
// (`protocol/typescript/positron/`) into the self-contained SDK
// (`sdk/typescript/generated/views/`).
//
// The SDK VENDORS these (copies, not cross-package imports) so a UI app depends
// only on `@continuum/sdk-typescript` and stays public-user-installable
// ([[headless-core-many-clients]], [[persona-is-a-client]]). Before this script the
// re-vendoring was manual ("do not hand-edit; re-vendor on change") and silently
// drifted — `ChatViewState.purpose` was hand-synced, `ForgeViewState` was never
// vendored. This is the #80 fix: ONE declared vendored set, copied deterministically.
//
//   node scripts/vendor-views.mjs           # re-vendor (copy protocol/ → SDK)
//   node scripts/vendor-views.mjs --check   # CI: fail loud if any copy drifted
//
// Adding a widget's view kind = add its name (and its transitive same-dir imports)
// to VENDORED below, then run it.

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..', '..'); // sdk/typescript/scripts → repo root
const SRC = join(root, 'protocol', 'typescript', 'positron');
const DEST = join(root, 'sdk', 'typescript', 'generated', 'views');

// The vendored closure: each view kind the SDK exposes + its transitive same-dir
// imports (verbatim copy works because every view imports its siblings by `./Name`).
const VENDORED = [
  // chat widget closure
  'ChatViewState',
  'ChatMessageView',
  'RosterSlotView',
  'SenderKind',
  'Provenance',
  // foundry widget closure
  'ForgeViewState',
  'ForgeModelView',
];

const check = process.argv.includes('--check');
let drift = 0;

for (const name of VENDORED) {
  const src = join(SRC, `${name}.ts`);
  const dest = join(DEST, `${name}.ts`);

  let canonical;
  try {
    canonical = readFileSync(src, 'utf8');
  } catch {
    console.error(
      `vendor-views: missing canonical source ${src} — regenerate it first ` +
        `(cargo test -p continuum-positron). Not vendoring a phantom type.`,
    );
    process.exit(1);
  }

  if (check) {
    let vendored = null;
    try {
      vendored = readFileSync(dest, 'utf8');
    } catch {
      vendored = null;
    }
    if (vendored !== canonical) {
      console.error(
        `vendor-views: DRIFT in views/${name}.ts — the vendored SDK copy differs ` +
          `from protocol/typescript/positron/. Run \`npm run vendor:views -w @continuum/sdk-typescript\`.`,
      );
      drift++;
    }
  } else {
    writeFileSync(dest, canonical);
    console.log(`vendored views/${name}.ts`);
  }
}

if (check) {
  if (drift > 0) process.exit(1);
  console.log(`vendor-views: ${VENDORED.length} view payloads in sync with protocol/ ✓`);
}
