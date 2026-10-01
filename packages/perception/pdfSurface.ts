/** PDF decoding is a Surface adapter, not a second perception pipeline.
 * Poppler supplies text and pixels from the same immutable file snapshot.
 * One page per observation keeps native vision and context costs bounded.
 */
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, open, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { imageDiff } from './imageDiff';
import { ActError, type Action, type Percept, type StructuredState, type Surface, type ViewSpec } from './surface';

const MAX_DOCUMENT_BYTES = 32 * 1024 * 1024;
const MAX_IMAGE_BYTES = 12 * 1024 * 1024;

export interface PdfSurfaceOptions {
  /** Local file URL, optionally #page=N (one-based). No remote fetch or browser download. */
  readonly target: string;
  /** Installed decoder executables; defaults resolve through the provider's PATH. */
  readonly executables?: { readonly info?: string; readonly text?: string; readonly render?: string };
}

export class PdfSurface implements Surface {
  private readonly abort = new AbortController();
  private observation?: { percept: Percept; structure: StructuredState };
  private closed = false;
  private rendering = false;

  private constructor(
    private readonly options: PdfSurfaceOptions,
    private readonly directory: string,
    private readonly snapshot: string,
    private readonly source: string,
    private readonly hash: string,
    private readonly page: number,
  ) {}

  static accepts(target: string): boolean {
    try {
      const url = new URL(target);
      return url.protocol === 'file:' && url.pathname.toLowerCase().endsWith('.pdf');
    } catch { return false; }
  }

  static async open(options: PdfSurfaceOptions): Promise<PdfSurface> {
    if (!PdfSurface.accepts(options.target)) throw new Error('PDF observation requires a local file:///...pdf URL');
    const url = new URL(options.target);
    const fragment = url.hash.slice(1);
    if (url.search || (fragment && !/^page=[1-9]\d*$/.test(fragment))) {
      throw new Error('PDF target accepts only #page=N, with a one-based page number');
    }
    const page = fragment ? Number(fragment.slice(5)) : 1;
    if (!Number.isSafeInteger(page)) throw new Error('Invalid PDF page number');
    url.hash = '';
    const source = fileURLToPath(url);
    const file = await open(source, 'r');
    let bytes: Buffer;
    try {
      const stat = await file.stat();
      if (!stat.isFile() || stat.size > MAX_DOCUMENT_BYTES) throw new Error('PDF must be a file of at most 32 MiB');
      // Bounded allocation even if the source grows while being read.
      bytes = Buffer.alloc(stat.size);
      let offset = 0;
      while (offset < bytes.length) {
        const result = await file.read(bytes, offset, bytes.length - offset, offset);
        if (!result.bytesRead) throw new Error('PDF changed while reading; retry the observation');
        offset += result.bytesRead;
      }
      const after = await file.stat();
      if (after.size !== stat.size || after.mtimeMs !== stat.mtimeMs) throw new Error('PDF changed while reading; retry the observation');
    } finally { await file.close(); }
    if (!bytes.subarray(0, 1024).includes(Buffer.from('%PDF-'))) throw new Error('Source is not a PDF');
    const directory = await mkdtemp(join(tmpdir(), 'continuum-pdf-'));
    try {
      const snapshot = join(directory, 'source.pdf');
      await writeFile(snapshot, bytes, { mode: 0o600 });
      return new PdfSurface(options, directory, snapshot, source, createHash('sha256').update(bytes).digest('hex'), page);
    } catch (error) {
      await rm(directory, { recursive: true, force: true });
      throw error;
    }
  }

  async render(view?: ViewSpec): Promise<Percept> {
    if (this.closed) throw new Error('PDF surface is closed');
    if (this.rendering) throw new Error('PDF observation already in progress');
    const width = view?.viewport?.width ?? 1440;
    const height = view?.viewport?.height ?? 1440;
    if (![width, height].every(n => Number.isInteger(n) && n > 0 && n <= 4096)) {
      throw new Error('PDF viewport dimensions must be integers between 1 and 4096');
    }
    this.rendering = true;
    this.observation = undefined;
    const deadline = Date.now() + 30_000;
    try {
      const info = (await this.run(this.options.executables?.info ?? 'pdfinfo', [this.snapshot], deadline, 1024 * 1024)).toString('utf8');
      const pages = Number(/^Pages:\s+(\d+)\s*$/m.exec(info)?.[1]);
      if (!Number.isSafeInteger(pages) || pages < 1) throw new Error('PDF decoder did not report a valid page count');
      if (this.page > pages) throw new Error(`PDF page ${this.page} is out of range (1-${pages})`);
      const range = ['-f', String(this.page), '-l', String(this.page)];
      const text = (await this.run(this.options.executables?.text ?? 'pdftotext', [...range, '-layout', '-enc', 'UTF-8', this.snapshot, '-'], deadline, 1024 * 1024)).toString('utf8').trim();
      const bytes = await this.run(this.options.executables?.render ?? 'pdftoppm', [...range, '-singlefile', '-png', '-scale-to', String(Math.min(width, height)), this.snapshot], deadline, MAX_IMAGE_BYTES);
      if (bytes.length < 24 || !bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) throw new Error('PDF renderer did not produce PNG pixels');
      const percept: Percept = { kind: 'image', mime: 'image/png', bytes, width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
      if (percept.width > 4096 || percept.height > 4096 || !percept.width || !percept.height) throw new Error('Rendered PDF page exceeds the 4096-pixel extent limit');
      this.observation = {
        percept,
        structure: {
          url: this.options.target,
          title: `${basename(this.source)} — page ${this.page}/${pages}`,
          tree: {
            tag: 'document', role: 'document', name: basename(this.source),
            attrs: { mime: 'application/pdf', source: this.source, sha256: this.hash, page: String(this.page), pages: String(pages) },
            children: [{ tag: 'page', text, attrs: { page: String(this.page), textLayer: text ? 'present' : 'empty; inspect page image (no OCR performed)' }, children: [] }],
          },
        },
      };
      return percept;
    } finally { this.rendering = false; }
  }

  async probe(): Promise<StructuredState> {
    if (this.closed || !this.observation) throw new Error('Render the PDF page before probing it');
    return this.observation.structure;
  }

  async act(_action: Action): Promise<void> { throw new ActError('PDF surface is read-only; observe a different #page=N to navigate'); }
  diff(before: Percept, after: Percept) { return imageDiff(before, after); }
  async close(): Promise<void> {
    this.closed = true;
    this.abort.abort();
    await rm(this.directory, { recursive: true, force: true });
  }

  private run(executable: string, args: string[], deadline: number, maxBuffer: number): Promise<Buffer> {
    const timeout = deadline - Date.now();
    if (timeout <= 0) return Promise.reject(new Error('PDF observation exceeded its 30-second deadline'));
    return new Promise((resolve, reject) => {
      execFile(executable, args, { encoding: 'buffer', windowsHide: true, timeout, maxBuffer, signal: this.abort.signal, env: { ...process.env, LC_ALL: 'C' } }, (error, stdout) => {
        if (error) reject(new Error(`PDF decoder ${executable} failed (${error.code ?? 'unknown'}): verify Poppler is installed on the eye-node and the PDF is readable`));
        else resolve(stdout);
      });
    });
  }
}
