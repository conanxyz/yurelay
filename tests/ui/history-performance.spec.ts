import { test, expect, type Page } from '@playwright/test';
import { defaults } from '../../src/types';

async function largeHistoryFixture(page: Page, repetitions = 1600, previousCount = 5, legacyList = false) {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.setViewportSize({ width: 1100, height: 820 });
  await page.addInitScript(({ settings, repetitions, previousCount, legacyList }) => {
    const w = window as any;
    w.isTauri = true;
    w.calls = [];
    w.measurements = [];
    w.operations = [];
    settings.ui_language = 'en';
    const source = 'Synthetic history source with distinct ordinary prose. '.repeat(850);
    const prose = 'A synthetic translation paragraph with enough words for a realistic reading block.\n\n'.repeat(repetitions);
    const oldTranslation = '| Column | Result |\n| --- | --- |\n' + '| synthetic row | preserved value |\n'.repeat(repetitions);
    const segment = { id: 'segment', kind: 'selection', original_text: source, captured_at: 100, asset_id: null, ocr_layout: null };
    const snapshot = { source_revision: 1, original_text: source, edited_text: source, source_language: 'en', target_language: 'zh-Hans', segments: [segment] };
    const records = Array.from({ length: 30 }, (_, index) => ({
      id: `record-${index}`, revision: 10, source_revision: 8, attempt: 6, source: 'selection',
      status: 'completed', original_text: `Record ${index}: ` + source, edited_text: source,
      translation: prose, profile: settings.profiles[0], target_language: 'zh-Hans', source_language: 'en', detected_language: 'en',
      created_at: 1791954000000 - index * 1000000, error: null, save_error: null,
      save_state: 'saved', has_image: false, image_pending: false, image_error: null,
      source_segments: [segment], source_history: Array.from({ length: 6 }, (_, n) => ({ ...snapshot, source_revision: n + 1 })),
      past_attempts: Array.from({ length: previousCount }, (_, n) => ({ number: n + 1, status: 'completed', profile: settings.profiles[0], translation: oldTranslation, source_text: source, source_revision: n + 1, timings: {}, error: null })), timings: {},
    }));
    w.records = records;
    const callbacks: Record<number, Function> = {};
    const events: Record<string, number[]> = {};
    w.emit = (name: string, payload: unknown) => (events[name] ?? []).forEach(id => callbacks[id]?.({ event: name, payload }));
    let sequence = 0;
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback(fn: Function) { callbacks[++sequence] = fn; return sequence; },
      convertFileSrc() { return "/synthetic-history-image.png"; },
      unregisterCallback(id: number) { delete callbacks[id]; },
      async invoke(cmd: string, args: any) {
        w.calls.push({ cmd, args });
        if (cmd === 'plugin:event|listen') { (events[args.event] ??= []).push(args.handler); return ++sequence; }
        if (cmd === 'bootstrap') return { configuration: { settings, has_keys: [], credential_errors: {} }, permissions: { accessibility: true, screen: true }, shortcuts: { registered: true }, snapshot: { revision: 0, tasks: [] }, selected: null, notice: null };
        if (cmd === 'permissions') return { accessibility: true, screen: true };
        if (cmd === 'service_verifications') return [];
        if (cmd === 'operation_list') return JSON.parse(JSON.stringify(w.operations));
        if (cmd === 'search_history') {
          const start = performance.now();
          const pageRows = records.slice(args.offset, args.offset + 30);
          const summaries = pageRows.map(record => ({ id: record.id, revision: record.revision, source: record.source, source_kinds: [...new Set(record.source_segments.map(segment => segment.kind))], status: record.status, original_text: Array.from(record.original_text).slice(0, 240).join(''), created_at: record.created_at }));
          const payload = JSON.stringify(legacyList ? pageRows : summaries);
          const result = JSON.parse(payload);
          w.measurements.push({ stage: 'list-json', bytes: new TextEncoder().encode(payload).length, ms: performance.now() - start });
          return result;
        }
        if (cmd === 'history_detail') return JSON.parse(JSON.stringify(records.find(item => item.id === args.id)));
        if (cmd === 'source_assets') return [];
        return null;
      },
    };
  }, { settings: structuredClone(defaults), repetitions, previousCount, legacyList });
  await page.goto('/?window=settings');
  await expect(page.getByLabel('Settings navigation')).toBeVisible();
}

test('opening a large history record does not render collapsed previous translations', async ({ page }) => {
  await largeHistoryFixture(page, Number(process.env.YURELAY_HISTORY_REPEATS ?? 1600), Number(process.env.YURELAY_HISTORY_PREVIOUS ?? 5), process.env.YURELAY_LEGACY_HISTORY_PAYLOAD === "1");
  const listStart = Date.now();
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await expect(page.locator('.history-row')).toHaveCount(30);
  const listMs = Date.now() - listStart;
  const start = Date.now();
  await page.locator('.history-row').first().click();
  await expect(page.locator('.history-detail-heading')).toBeVisible();
  await expect(page.locator('.detail-translation .markdown-reading')).toBeVisible();
  await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  const detailMs = Date.now() - start;
  const collapsedRenderers = await page.locator('.history-attempts:not([open]) .markdown-reading').count();
  const nodes = await page.locator('.history-detail *').count();
  console.log(JSON.stringify({ listMs, detailMs, collapsedRenderers, nodes, transport: await page.evaluate(() => (window as any).measurements) }));
  expect(collapsedRenderers, 'closed previous translations must not parse/render Markdown before expansion').toBe(0);
  expect(detailMs, 'a historical record must become interactive within one second for this synthetic fixture').toBeLessThan(1000);
  const previousCount = Number(process.env.YURELAY_HISTORY_PREVIOUS ?? 5);
  if (previousCount) {
    await page.locator('.history-attempts > summary').click();
    await expect(page.locator('.history-attempts .markdown-reading')).toHaveCount(previousCount);
  }
});

async function useSmallHistory(page: Page) {
  await largeHistoryFixture(page, 10, 1);
  await page.evaluate(() => (window as any).records.forEach((record: any) => {
    record.original_text = record.original_text.slice(0, 3050);
    record.edited_text = null;
    record.translation = record.translation.slice(0, 1732);
    record.source_segments = [];
    record.source_history = [];
    record.past_attempts = [];
  }));
}

test('one window activation refreshes history and selected detail once', async ({ page }) => {
  await useSmallHistory(page);
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.locator('.history-row').first().click();
  await expect(page.locator('.history-detail-heading')).toBeVisible();
  await page.evaluate(() => { const w = window as any; w.records[0].revision++; w.calls = []; window.dispatchEvent(new Event('focus')); w.emit('settings-focused', null); });
  await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  await expect.poll(() => page.evaluate(() => (window as any).calls.filter((call: any) => call.cmd === 'history_detail').length)).toBe(1);
  const counts = await page.evaluate(() => { const calls = (window as any).calls; return { lists: calls.filter((call: any) => call.cmd === 'search_history').length, details: calls.filter((call: any) => call.cmd === 'history_detail').length }; });
  console.log(JSON.stringify({ activation: counts }));
  expect(counts).toEqual({ lists: 1, details: 1 });
  await page.evaluate(() => { const w = window as any; w.calls = []; window.dispatchEvent(new Event('focus')); w.emit('settings-focused', null); });
  await expect.poll(() => page.evaluate(() => (window as any).calls.filter((call: any) => call.cmd === 'search_history').length)).toBe(1);
  expect(await page.evaluate(() => (window as any).calls.filter((call: any) => call.cmd === 'history_detail').length)).toBe(0);
});

test('history entrance settles promptly with ordinary sized local records', async ({ page }) => {
  await useSmallHistory(page);
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await expect(page.locator('.history-row')).toHaveCount(30);
  const durations = await page.locator('.history-panel').evaluate(element => element.getAnimations().map(animation => Number(animation.effect?.getTiming().duration ?? 0)));
  console.log(JSON.stringify({ historyEntranceDurationsMs: durations }));
  expect(Math.max(0, ...durations)).toBeLessThanOrEqual(200);
});


test('explanation and reply history only renders the expanded operation', async ({ page }) => {
  await useSmallHistory(page);
  await page.evaluate(() => {
    const w = window as any;
    w.operations = Array.from({ length: 5 }, (_, index) => ({
      id: `operation-${index}`, request_id: `request-${index}`, task_id: null, source_revision: 1, kind: 'reply', status: 'completed', primary_text: 'Synthetic saved reply.\n\n'.repeat(600), back_translation: '', back_translation_state: 'none', profile: w.records[0].profile, attempt: 1, attempts: [], error: null, sensitive: null, output_language: 'en', reference_language: 'zh-Hans', created_at: 1000-index, updated_at: 1000-index, revision: 1, memory_only: false, save_error: null,
    }));
  });
  await page.getByRole('button', { name: 'History', exact: true }).click();
  expect(await page.evaluate(() => (window as any).calls.filter((call: any) => call.cmd === 'operation_list').length)).toBe(0);
  await page.locator('.operation-history > summary').click();
  await expect(page.locator('.operation-history-record')).toHaveCount(5);
  await expect(page.locator('.operation-output')).toHaveCount(0);
  await page.locator('.operation-history-record > summary').first().click();
  await expect(page.locator('.operation-output')).toHaveCount(1);
  await page.locator('.operation-history-record > summary').first().click();
  await expect(page.locator('.operation-output')).toHaveCount(0);
});


test('closed historical screenshots do not create image requests before expansion', async ({ page }) => {
  await useSmallHistory(page);
  let requests = 0;
  await page.route('**/synthetic-history-image.png', async route => { requests++; await route.fulfill({ status: 200, contentType: 'image/png', body: Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aNd0AAAAASUVORK5CYII=', 'base64') }); });
  await page.evaluate(() => { const record = (window as any).records[0]; record.has_image = true; record.source = 'screenshot'; });
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.locator('.history-row').first().click();
  await expect(page.locator('.history-image')).toBeVisible();
  await expect(page.locator('.history-image:not([open]) img')).toHaveCount(0);
  expect(requests).toBe(0);
  await page.locator('.history-image > summary').click();
  await expect(page.locator('.history-image img')).toBeVisible();
  await expect.poll(() => requests).toBe(1);
});

test('ordinary text history details do not issue screenshot asset requests', async ({ page }) => {
  await useSmallHistory(page);
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.locator('.history-row').first().click();
  await expect(page.locator('.history-detail-heading')).toBeVisible();
  expect(await page.evaluate(() => (window as any).calls.filter((call: any) => call.cmd === 'source_assets').length)).toBe(0);
});
