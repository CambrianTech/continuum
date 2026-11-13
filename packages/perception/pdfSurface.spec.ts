import { afterEach, expect, it } from 'vitest';
import { mkdtemp, open, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { PdfSurface } from './pdfSurface';

const directories: string[] = [];
afterEach(async () => {
  for (const path of directories.splice(0)) await rm(path, { recursive: true, force: true });
});

// A file URL must not quietly turn document inspection into a network-share read.
it('rejects remote file hosts before filesystem access', async () => {
  await expect(PdfSurface.open({ target: 'file://unreachable.invalid/share/resume.pdf' }))
    .rejects.toThrow('not a network file host');
});

// Reject source and render allocations before invoking any installed decoder.
it('bounds local source size and rendering requests independently of Poppler availability', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'pdf-surface-test-'));
  directories.push(directory);
  const path = join(directory, 'document.pdf');
  const target = pathToFileURL(path).href;
  const file = await open(path, 'w');
  try { await file.truncate(32 * 1024 * 1024 + 1); } finally { await file.close(); }
  await expect(PdfSurface.open({ target })).rejects.toThrow('at most 32 MiB');
  await writeFile(path, 'not a PDF');
  await expect(PdfSurface.open({ target })).rejects.toThrow('not a PDF');
  await writeFile(path, '%PDF-1.4\n');
  const surface = await PdfSurface.open({ target });
  try {
    await expect(surface.render({ viewport: { width: 999999, height: 1440 } }))
      .rejects.toThrow('between 1 and 4096');
    await expect(surface.act({ kind: 'click' })).rejects.toThrow('read-only');
    await expect(surface.probe()).rejects.toThrow('Render the PDF page');
  } finally { await surface.close(); }
  await expect(surface.render()).rejects.toThrow('closed');
});
