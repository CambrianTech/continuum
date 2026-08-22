// Re-vendor continuum view payloads from the canonical ts-rs output
// (`protocol/typescript/*`) into the self-contained SDK (`sdk/typescript/generated/*`).
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
// Adding a widget's view kind = add its name (with `src`/`dest` dirs, and its
// transitive same-dir + cross-dir imports) to VENDORED below, then run it.
//
// `dest` MIRRORS `src` so a type's RELATIVE imports (`./Sibling`, `../grid/TrustLevel`)
// resolve identically in the vendored copy. The Experience manifest closure therefore
// keeps its own `experience/` + `grid/` dirs rather than flattening; positron payloads
// keep their historical `views/` dest name.

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..', '..'); // sdk/typescript/scripts → repo root
const protoDir = (sub) => join(root, 'protocol', 'typescript', sub);
const genDir = (sub) => join(root, 'sdk', 'typescript', 'generated', sub);

const VENDORED = [
  // chat widget closure (positron payloads → views)
  { src: 'positron', dest: 'views', name: 'ChatViewState' },
  { src: 'positron', dest: 'views', name: 'ChatMessageView' },
  // ChatViewState.acts — the transcript's tool-act receipt stream (#243)
  { src: 'positron', dest: 'views', name: 'ActReceiptView' },
  { src: 'positron', dest: 'views', name: 'RosterSlotView' },
  // RosterSlotView imports ./Loadout (the model·size·ctx strip, #186)
  { src: 'positron', dest: 'views', name: 'Loadout' },
  { src: 'positron', dest: 'views', name: 'SenderKind' },
  { src: 'positron', dest: 'views', name: 'Provenance' },
  // foundry widget closure
  { src: 'positron', dest: 'views', name: 'ForgeViewState' },
  { src: 'positron', dest: 'views', name: 'ForgeModelView' },
  // roster widget kind — path-3 per-region ViewState; imports ./RosterSlotView (views)
  { src: 'positron', dest: 'views', name: 'RosterViewState' },
  // nav closure (kind="nav" — a citizen's open tabs / unread / bookmarks)
  { src: 'positron', dest: 'views', name: 'NavViewState' },
  { src: 'positron', dest: 'views', name: 'NavTab' },
  { src: 'positron', dest: 'views', name: 'NavBookmark' },
  { src: 'positron', dest: 'views', name: 'NavTargetKind' },
  // system-metrics closure (kind="system-metrics" — the SYS gauge's series)
  { src: 'positron', dest: 'views', name: 'SystemMetricsViewState' },
  { src: 'positron', dest: 'views', name: 'MetricSeriesView' },
  // serving closure (kind="serving" — the pager glass box, #141 slice 1)
  { src: 'positron', dest: 'views', name: 'ServingViewState' },
  { src: 'positron', dest: 'views', name: 'ServingHeaderView' },
  { src: 'positron', dest: 'views', name: 'ServingArmView' },
  { src: 'positron', dest: 'views', name: 'ServingEventCard' },
  // bench closure (kind="bench" — the academy's live benchmark board, #329)
  { src: 'positron', dest: 'views', name: 'BenchViewState' },
  { src: 'positron', dest: 'views', name: 'BenchRunRow' },
  { src: 'positron', dest: 'views', name: 'BenchRoundRow' },
  // kanban closure (kind="kanban" — the work board; the persona home's claims
  // feed renders cards by assignee). Vendored now that a widget renders it.
  { src: 'positron', dest: 'views', name: 'KanbanViewState' },
  { src: 'positron', dest: 'views', name: 'KanbanCardView' },
  { src: 'positron', dest: 'views', name: 'KanbanLaneView' },
  { src: 'positron', dest: 'views', name: 'KanbanCardState' },
  { src: 'positron', dest: 'views', name: 'KanbanLaneState' },
  { src: 'positron', dest: 'views', name: 'KanbanPriority' },
  { src: 'positron', dest: 'views', name: 'KanbanPullRequest' },
  // KanbanCardView imports ./KanbanHold (lease liveness, #321/#331)
  { src: 'positron', dest: 'views', name: 'KanbanHold' },
  // Experience / Join Contract manifest closure — mirrored into experience/, keeps
  // its `./Sibling` + `../grid/TrustLevel` relative imports.
  { src: 'experience', dest: 'experience', name: 'Experience' },
  { src: 'experience', dest: 'experience', name: 'Region' },
  { src: 'experience', dest: 'experience', name: 'RegionRole' },
  { src: 'experience', dest: 'experience', name: 'RegionScope' },
  { src: 'experience', dest: 'experience', name: 'Affordance' },
  { src: 'experience', dest: 'experience', name: 'ProofSpec' },
  { src: 'experience', dest: 'experience', name: 'Member' },
  { src: 'experience', dest: 'experience', name: 'Standing' },
  { src: 'experience', dest: 'experience', name: 'Layout' },
  { src: 'experience', dest: 'experience', name: 'LayoutChild' },
  // cross-dir dep of Affordance (`../grid/TrustLevel`)
  { src: 'grid', dest: 'grid', name: 'TrustLevel' },
];

const check = process.argv.includes('--check');
let drift = 0;

for (const { src, dest, name } of VENDORED) {
  const srcPath = join(protoDir(src), `${name}.ts`);
  const destPath = join(genDir(dest), `${name}.ts`);

  let canonical;
  try {
    canonical = readFileSync(srcPath, 'utf8');
  } catch {
    console.error(
      `vendor-views: missing canonical source ${srcPath} — regenerate it first ` +
        `(cargo test -p continuum-positron / continuum-core). Not vendoring a phantom type.`,
    );
    process.exit(1);
  }

  if (check) {
    let vendored = null;
    try {
      vendored = readFileSync(destPath, 'utf8');
    } catch {
      vendored = null;
    }
    if (vendored !== canonical) {
      console.error(
        `vendor-views: DRIFT in ${dest}/${name}.ts — the vendored SDK copy differs ` +
          `from protocol/typescript/${src}/. Run \`npm run vendor:views -w @continuum/sdk-typescript\`.`,
      );
      drift++;
    }
  } else {
    mkdirSync(genDir(dest), { recursive: true });
    writeFileSync(destPath, canonical);
    console.log(`vendored ${dest}/${name}.ts`);
  }
}

if (check) {
  if (drift > 0) process.exit(1);
  console.log(`vendor-views: ${VENDORED.length} view payloads in sync with protocol/ ✓`);
}
