import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { readFile, writeFile, mkdir, readdir } from 'node:fs/promises';
import { dirname, extname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const require = createRequire(join(root, 'crates/diff/web/package.json'));
const { chromium } = require('playwright');
const output = resolve(process.argv[2] ?? join(root, 'target/migration-baseline/browser'));
const fixtures = resolve(process.argv[3] ?? join(root, 'target/migration-baseline/fixtures'));
const dist = join(root, 'crates/diff/web/dist');
const samples = Number(process.env.BASELINE_SAMPLES ?? 5);
assert(Number.isInteger(samples) && samples > 0, 'BASELINE_SAMPLES must be a positive integer');
await mkdir(join(output, 'screenshots'), { recursive: true });
const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm' };
const server = createServer(async (request, response) => {
  try {
    const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
    const path = resolve(dist, `.${pathname === '/' ? '/index.html' : pathname}`);
    if (!path.startsWith(`${dist}${sep}`)) throw new Error('invalid asset path');
    response.setHeader('Content-Type', types[extname(path)] ?? 'application/octet-stream');
    response.end(await readFile(path));
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((ready) => server.listen(0, '127.0.0.1', ready));
const url = `http://127.0.0.1:${server.address().port}/`;
let browserServer;
let browser;
const errors = [];
const logs = [];
const metrics = [];
const screenshots = [];
const events = [];
const hashes = {};
let requestId = 0;
try {
  browserServer = await chromium.launchServer({ headless: true });
  browser = await chromium.connect(browserServer.wsEndpoint());
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  await context.addInitScript(() => {
    window.addEventListener('TrunkApplicationStarted', (event) => {
      window.baselineWasm = event.detail.wasm;
    });
  });
  const page = await context.newPage();
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => logs.push({ type: message.type(), text: message.text() }));
  const cdp = await context.newCDPSession(page);
  await cdp.send('Performance.enable');
  const startup = performance.now();
  await page.goto(url);
  await page.waitForFunction(() => window.wasmBindings && document.querySelector('canvas'), null, { timeout: 60_000 });
  await settle(page);
  const startupMs = performance.now() - startup;
  await page.evaluate(() => {
    window.baselineEvents = [];
    for (const name of [
      'diff-review-document-applied', 'diff-review-command-result', 'diff-review-submit',
      'diff-review-copy', 'diff-review-cancel', 'markdown-review-submit', 'markdown-review-copy',
      'markdown-review-cancel', 'diff-review-repository-action', 'diff-review-refresh',
    ]) document.addEventListener(name, (event) => window.baselineEvents.push({ name, detail: event.detail }));
  });
  async function fixture(name) {
    const bytes = await readFile(join(fixtures, `${name}.json`));
    hashes[`${name}.json`] = createHash('sha256').update(bytes).digest('hex');
    return JSON.parse(bytes);
  }
  async function push(document) {
    return await page.evaluate(async (document) => {
      const started = performance.now();
      const ack = await new Promise((resolve, reject) => {
        const listener = (event) => { clearTimeout(timer); resolve(JSON.parse(event.detail)); };
        const timer = setTimeout(() => { documentGlobal.removeEventListener('diff-review-document-applied', listener); reject(new Error('document acknowledgement timeout')); }, 30_000);
        const documentGlobal = window.document;
        documentGlobal.addEventListener('diff-review-document-applied', listener, { once: true });
        try { window.wasmBindings.set_document_json(JSON.stringify(document)); }
        catch (error) { clearTimeout(timer); documentGlobal.removeEventListener('diff-review-document-applied', listener); reject(error); }
      });
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      return { acknowledgement: ack, elapsed_ms: performance.now() - started };
    }, document);
  }
  async function command(command, target = 'diff') {
    const id = ++requestId;
    return await page.evaluate(async ({ command, target, id }) => {
      const started = performance.now();
      const result = await new Promise((resolve, reject) => {
        const listener = (event) => {
          const result = JSON.parse(event.detail);
          if (result.request_id !== id) return;
          clearTimeout(timer);
          document.removeEventListener('diff-review-command-result', listener);
          resolve(result);
        };
        const timer = setTimeout(() => { document.removeEventListener('diff-review-command-result', listener); reject(new Error('command result timeout')); }, 30_000);
        document.addEventListener('diff-review-command-result', listener);
        try { window.wasmBindings.dispatch_command_json(JSON.stringify({ request_id: id, target, command })); }
        catch (error) { clearTimeout(timer); document.removeEventListener('diff-review-command-result', listener); reject(error); }
      });
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      return { ...result, elapsed_ms: performance.now() - started };
    }, { command, target, id });
  }
  async function handled(value, target = 'diff') {
    const result = await command(value, target);
    assert.equal(result.error, null);
    assert.equal(result.handled, true, JSON.stringify(value));
    return result;
  }
  async function shot(name) {
    await settle(page);
    const path = join(output, 'screenshots', `${name}.png`);
    await page.screenshot({ path, animations: 'disabled' });
    const bytes = await readFile(path);
    screenshots.push({ name, file: `screenshots/${name}.png`, sha256: createHash('sha256').update(bytes).digest('hex'), bytes: bytes.length });
  }
  const small = await fixture('small');
  const replacement = await fixture('same-count-refresh');
  assert.equal((await push(small)).acknowledgement.changed, true);
  await page.evaluate(() => window.wasmBindings.set_theme('sage'));
  await handled({ set_view_mode: 'Unified' });
  await shot('diff-unified');
  await handled({ set_view_mode: 'Split' });
  await shot('diff-split');
  await handled({ focus: 'diff' });
  await handled('first');
  await handled({ move_selection: 1 });
  await handled({ review: 'begin_comment' });
  await page.keyboard.insertText('Check Unicode, tabs, anchors, and wrapped review comments.\nA second line must survive export.');
  await shot('comment-draft');
  await handled({ review: 'submit_comment' });
  await shot('comment-thread');
  await handled('copy_review');
  await handled('submit_review');
  await handled({ review: 'open_theme_picker' });
  await shot('theme-picker');
  await handled({ review: 'cancel' });
  await page.evaluate(() => window.wasmBindings.set_theme('ayu-dark'));
  await shot('theme-ayu-dark');
  await page.keyboard.press('Control+=');
  await page.keyboard.press('Control+=');
  await shot('interface-size-20');
  await page.keyboard.press('Control+0');
  const markdown = await readFile(join(root, 'tests/fixtures/migration/review.md'), 'utf8');
  hashes['review.md'] = createHash('sha256').update(markdown).digest('hex');
  await page.evaluate((source) => window.wasmBindings.set_markdown_document_json(JSON.stringify({ source, source_path: 'review.md', title: 'Migration acceptance' })), markdown);
  await shot('markdown');
  await handled('approve', 'markdown');
  await push(small);
  const performanceFixtures = [['small', small], ['large', await fixture('large')], ['many-files', await fixture('many-files')], ['long-lines', await fixture('long-lines')]];
  for (const [name, document] of performanceFixtures) {
    await push(document);
    await handled({ set_view_mode: 'Unified' });
    await page.waitForTimeout(1000);
    const memory = await memorySample(cdp, page, browserServer.process().pid);
    const layout = [];
    const scrolling = [];
    const refresh = [];
    for (let i = 0; i < samples; i++) {
      layout.push((await handled({ set_view_mode: i % 2 ? 'Unified' : 'Split' })).elapsed_ms);
      scrolling.push((await handled({ scroll: { pane: 'diff', lines: 25 } })).elapsed_ms);
      const next = name === 'small' ? (i % 2 ? small : replacement) : (i % 2 ? document : refreshVariant(document));
      refresh.push((await push(next)).elapsed_ms);
    }
    metrics.push({ fixture: name, memory, layout_boundary_ms: layout, scroll_boundary_ms: scrolling, refresh_boundary_ms: refresh });
  }
  events.push(...await page.evaluate(() => window.baselineEvents));
  assert(events.some((event) => event.name === 'diff-review-copy' && event.detail.includes('A second line')));
  assert(events.some((event) => event.name === 'diff-review-submit'));
  assert(events.some((event) => event.name === 'markdown-review-submit'));
  assert.deepEqual(errors, []);
  assert.deepEqual(logs.filter((entry) => entry.type === 'error'), []);
  const environment = await page.evaluate(() => {
    const gl = document.createElement('canvas').getContext('webgl2');
    const extension = gl?.getExtension('WEBGL_debug_renderer_info');
    return {
      user_agent: navigator.userAgent,
      viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
      webgpu_available: Boolean(navigator.gpu),
      webgl_renderer_probe: extension ? gl.getParameter(extension.UNMASKED_RENDERER_WEBGL) : null,
      note: 'The WebGL probe identifies available Chromium graphics, not necessarily the canvas renderer backend.',
    };
  });
  const assets = {};
  for (const name of await readdir(dist)) {
    const bytes = await readFile(join(dist, name));
    assets[name] = { bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
  }
  await writeFile(join(output, 'results.json'), JSON.stringify({
    schema_version: 1,
    captured_at: new Date().toISOString(),
    source_revision: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    tracked_changes_at_capture: execFileSync('git', ['diff', 'HEAD', '--name-only'], { cwd: root, encoding: 'utf8' }).trim().split('\n').filter(Boolean),
    capture_script_sha256: createHash('sha256').update(await readFile(fileURLToPath(import.meta.url))).digest('hex'),
    profile: 'trunk build --release', browser: await browser.version(), environment, assets, fixture_sha256: hashes,
    sample_count: samples, memory_settle_ms: 1000, startup_to_ready_and_settle_ms: startupMs, screenshots, measurements: metrics,
    limitations: [
      'Boundary latency includes command/JSON processing and two requestAnimationFrame callbacks; it is not isolated CPU frame time or GPU presentation time.',
      'Startup is navigation to readiness, two animation frames and 150ms settle in a fresh browser context; it is not a native startup measurement and OS caches are not cleared.',
      'Memory is renderer JS heap, WASM linear memory and aggregate dedicated Chromium process-tree RSS; RSS includes shared pages and is not native Diff RSS.',
      'Memory is sampled after a fixed one-second settle, without forcing GC; long-lived steady state is not established.',
      'GPUI test-window node counts and native/macOS window behavior are not measured by this browser capture.',
      'Generated large/many-file fixtures have patch-only sources as provided by DocumentBuilder::generated.',
    ], errors,
  }, null, 2) + '\n');
  await writeFile(join(output, 'observed-events.json'), JSON.stringify(events, null, 2) + '\n');
  await writeFile(join(output, 'console.json'), JSON.stringify(logs, null, 2) + '\n');
  console.log(`Captured ${screenshots.length} screenshots and ${metrics.length} fixture measurements in ${output}`);
} finally {
  if (errors.length) console.error(errors);
  if (logs.some((entry) => entry.type === 'error')) console.error(logs.filter((entry) => entry.type === 'error'));
  await browser?.close();
  await browserServer?.close();
  await new Promise((closed) => server.close(closed));
}

async function settle(page) {
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await page.waitForTimeout(150);
}

async function memorySample(cdp, page, rootPid) {
  const metrics = await cdp.send('Performance.getMetrics');
  const js = Object.fromEntries(metrics.metrics.filter(({ name }) => ['JSHeapUsedSize', 'JSHeapTotalSize'].includes(name)).map(({ name, value }) => [name, value]));
  const wasm = await page.evaluate(() => window.baselineWasm?.memory?.buffer.byteLength ?? null);
  let rss = null;
  try {
    const records = [];
    for (const entry of await readdir('/proc')) {
      if (!/^\d+$/.test(entry)) continue;
      try {
        const status = await readFile(`/proc/${entry}/status`, 'utf8');
        records.push({ pid: Number(entry), parent: Number(status.match(/^PPid:\s+(\d+)/m)?.[1]), rss: Number(status.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0) * 1024 });
      } catch {}
    }
    const pids = new Set([rootPid]);
    let count;
    do { count = pids.size; for (const entry of records) if (pids.has(entry.parent)) pids.add(entry.pid); } while (count !== pids.size);
    rss = records.filter((entry) => pids.has(entry.pid)).reduce((sum, entry) => sum + entry.rss, 0);
  } catch {}
  return { ...js, wasm_linear_bytes: wasm, chromium_process_tree_rss_bytes: rss };
}

function refreshVariant(document) {
  const file = document.files[0];
  const hunk = file.hunks[0];
  const index = hunk.lines.findIndex((line) => line.kind === 'Added');
  assert(index >= 0, 'performance fixture needs an added line');
  const original = hunk.lines[index].text;
  const text = `${original} refreshed`;
  const changed = {
    ...file,
    hunks: [{ ...hunk, lines: hunk.lines.map((line, i) => i === index ? { ...line, text } : line) }, ...file.hunks.slice(1)],
  };
  if (file.new_source?.Ok) changed.new_source = { Ok: file.new_source.Ok.replace(original, text) };
  return { ...document, files: [changed, ...document.files.slice(1)] };
}
