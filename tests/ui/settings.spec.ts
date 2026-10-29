import { test, expect, type Page } from '@playwright/test';
import { defaults } from '../../src/types';

async function recordAppendShortcut(page: Page, kind: 'selection' | 'screenshot', key: string) {
 const button=page.getByRole('button',{name:`Record append ${kind} shortcut`,exact:true});
 await button.click();await expect(button).toHaveClass(/recording/);
 await page.keyboard.press(`Alt+Shift+${key}`);
 await expect(button).not.toHaveClass(/recording/);
 await expect(button.locator('kbd')).toHaveText(['⌥','⇧',key]);
}


test('authorizing an existing key preserves unsaved service edits and never saves a new key', async ({ page }) => {
  await settingsFixture(page);
  await page.getByRole('button',{name:'Services',exact:true}).click();
  await page.getByLabel('Service name',{exact:true}).fill('Unsaved synthetic service name');
  await page.getByRole('button',{name:'Authorize saved key',exact:true}).click();
  await expect(page.getByText('Saved key access authorized',{exact:true})).toBeVisible();
  await expect(page.getByLabel('Service name',{exact:true})).toHaveValue('Unsaved synthetic service name');
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='authorize_saved_credential').map((c:any)=>c.args))).toEqual([{profileId:'default'}]);
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='save_configuration').length)).toBe(0);
});

async function settingsFixture(page: Page, delayLoad = false, initialName = 'Custom service', withBackup = false) {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.setViewportSize({ width: 1100, height: 820 });
  await page.addInitScript((initial) => {
    const w = window as any;
    w.isTauri = true;
    w.calls = [];
    w.permissions = { accessibility: true, screen: true };
    w.savedKeys=["default"];
    w.vocabulary=[{id:'word-1',word:'resilient',phonetic:'/rɪˈzɪliənt/',pos:'adj.',dict_meaning:'able to recover quickly',note:'',difficulty:'B2',status:'learning',review_count:3,created_at:1792000000000,updated_at:1792000000000,contexts:[{id:'context-1',surface:'resilient',context_meaning:'坚韧的',context_sentence:'A resilient team adapts to change.',sentence_trans:'坚韧的团队能够适应变化。',created_at:1792000000000}]}];
    w.settings = initial.settings;
    w.settings.ui_language = 'en';
    w.settings.profiles[0].name = initial.initialName;
    w.settings.profiles[0].model = 'fixture-model';
    w.settings.profiles[0].endpoint = 'https://fixture.example/v1';
    if(initial.withBackup)w.settings.profiles.push({...structuredClone(w.settings.profiles[0]),id:'backup',name:'Backup service',model:'backup-model'});
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    let callback = 0;
    const callbacks: Record<number, Function> = {}, events: Record<string, number> = {};
    w.emit = (name: string, payload: unknown) => callbacks[events[name]]?.({ event: name, payload });
    w.closeSettings = (intent = 'close') => w.emit('settings-close-requested', { id: 1, intent });
    w.records = Array.from({ length: 30 }, (_, index) => ({
      id: `record-${index}`, revision: 1, attempt: 1, source: index % 2 ? 'screenshot' : 'selection',
      status: 'completed', original_text: `Record ${index}: A complete source paragraph with enough words to wrap across the list.`,
      edited_text: null, translation: `第 ${index} 条记录的译文。\n` + '这是完整内容。'.repeat(40),
      profile: w.settings.profiles[0], target_language: 'zh-Hans', source_language: null,
      detected_language: 'en', created_at: 1791954000000 - index * 1000000,
      error: null, save_state: 'saved', has_image: false, past_attempts: [], timings: {},
    }));
    const config = () => ({ settings: structuredClone(w.settings), has_keys: [...w.savedKeys], credential_errors: {} });
    w.__TAURI_INTERNALS__ = {
      transformCallback: (fn: Function) => { callbacks[++callback] = fn; return callback; },
      unregisterCallback: (id: number) => { delete callbacks[id]; },
      invoke: async (cmd: string, args: any) => {
        w.calls.push({ cmd, args });
        if (cmd === 'plugin:event|listen') { events[args.event] = args.handler; return callback; }
        if (cmd === 'bootstrap') {
          if(initial.delayLoad)await new Promise<void>(resolve=>w.releaseBootstrap=resolve);
          return { configuration: config(), permissions: structuredClone(w.permissions), shortcuts: { registered: true, error: null }, snapshot: { revision: 0, tasks: [] }, selected: null, notice: null };
        }
        if (cmd === 'save_configuration') {
          if (w.failSave) throw 'Fixture save failed';
          w.settings = structuredClone(args.settings);
          if(w.invalidateVerificationsOnSave)w.verifications=[];
          for(const [id,value] of Object.entries(args.apiKeyUpdates??{})) {if(value===null)w.savedKeys=w.savedKeys.filter((key:string)=>key!==id);else if(!w.savedKeys.includes(id))w.savedKeys.push(id);}
          return { configuration: config(), shortcuts: { registered: true, error: null } };
        }
        if(cmd==='remove_task'){w.records=w.records.filter((r:any)=>r.id!==args.id);w.operations=(w.operations??[]).filter((o:any)=>o.task_id!==args.id);return null;}
        if(cmd==='default_reply_prompt')return 'Use the source context to express the user intent in natural language for a native reader. Never invent facts or commitments.';
        if(cmd==='operation_list')return structuredClone(w.operations??[]);
        if(cmd==='service_verifications')return structuredClone(w.verifications??[]);
        if(cmd==='test_connection'){if(w.deferTest)await new Promise<void>(resolve=>w.finishTest=resolve);if(w.testError)throw w.testError;if(w.receiptOnTest&&JSON.stringify(args.profile)===JSON.stringify(w.settings.profiles.find((p:any)=>p.id===args.profile.id))&&!args.keyOverride)w.verifications=[{profile_id:args.profile.id,state:'verified',tested_at:Date.now(),expires_at:Date.now()+86400000,tested_capability:'translation_transport'}];return {first_content_ms:1,total_ms:2};}
        if (cmd === 'take_settings_route') { const route=w.pendingRoute;w.pendingRoute=null;return route??null; }
        if (cmd === 'permissions') return structuredClone(w.permissions);
        if(cmd==='vocabulary_list')return structuredClone(w.vocabulary);
        if(cmd==='fetch_models')return [{id:'fixture/free-model:free',free:true},{id:'fixture/zero-input-paid-output',free:false},{id:'fixture/unknown',free:null}];
        if(cmd==='vocabulary_update'){const item=w.vocabulary.find((i:any)=>i.id===args.id);if(args.note!==undefined)item.note=args.note;if(args.status)item.status=args.status;if(args.removeContext)item.contexts=item.contexts.filter((c:any)=>c.id!==args.removeContext);return structuredClone(item)}
        if(cmd==='vocabulary_remove'){w.vocabulary=w.vocabulary.filter((i:any)=>i.id!==args.id);return null}
        if(cmd==='vocabulary_export')return true;
        if (cmd === 'search_history') return args.query==='no match'?[]:w.records.slice(args.offset, args.offset + 30);
        if (cmd === 'history_detail') {
          const record=w.records.find((r: any) => r.id === args.id);
          if(w.delayDetail)await new Promise<void>(resolve=>w.releaseDetail=resolve);
          return record;
        }
        if (cmd === 'begin_shortcut_recording') return 1;
        if (cmd === 'end_shortcut_recording') {if(w.deferEndRecording)await new Promise(resolve=>w.releaseRecording=resolve);return { registered: true, error: null };}
        return null;
      },
    };
  }, { settings: structuredClone(defaults), delayLoad, initialName, withBackup });
  await page.goto('/?window=settings');
  if(delayLoad){await expect.poll(()=>page.evaluate(()=>(window as any).calls.some((c:any)=>c.cmd==='settings_guard_ready'))).toBe(true);return;}
  await expect(page.getByRole('button', { name: 'Save changes', exact: true })).toBeDisabled();
  await expect(page.getByLabel('Settings navigation')).toBeVisible();
  await expect.poll(()=>page.evaluate(()=>(window as any).calls.some((c:any)=>c.cmd==='settings_guard_ready'))).toBe(true);
}

test('auto-hide starts enabled and an explicit opt-out can be reverted or saved', async ({ page }) => {
  await settingsFixture(page);
  await page.getByRole('button', { name: 'Appearance', exact: true }).click();
  const autoHide = page.getByRole('checkbox', {
    name: 'Hide the translation window when another app gains focus', exact: true,
  });
  await expect(autoHide).toBeChecked();
  await autoHide.uncheck();
  expect(await page.evaluate(() => (window as any).settings.auto_hide)).toBe(true);
  await page.getByRole('button', { name: 'Revert', exact: true }).click();
  await expect(autoHide).toBeChecked();
  await autoHide.uncheck();
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  expect(await page.evaluate(() => (window as any).settings.auto_hide)).toBe(false);
  await expect(autoHide).not.toBeChecked();
});

test('default source and target languages sit side by side and save together', async ({ page }) => {
  await settingsFixture(page);
  const source = page.getByLabel('Default source language', { exact: true });
  const target = page.getByLabel('Default translation language', { exact: true });
  await source.selectOption('en');
  await target.selectOption('fr');
  const [left,right] = await page.evaluate(()=>Array.from(document.querySelectorAll('.default-languages select')).map(el=>el.getBoundingClientRect().toJSON()));
  expect(left!.x + left!.width).toBeLessThan(right!.x);
  expect(left!.y).toBe(right!.y);
  await expect(page.getByText('Review first. Translate when ready.', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Language and display preferences save automatically across all windows.', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Record a key combination, then save to apply it.', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Shortcuts are registered. Save to apply changes.', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Required to read selected text', { exact: true })).toHaveCount(0);
  await expect(page.getByText('click to record key', { exact: true })).toHaveCount(2);
  expect(await page.evaluate(() => (window as any).calls.some((c: any) => c.cmd === 'save_preferences'))).toBe(false);
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  expect(await page.evaluate(() => ({ source: (window as any).settings.source_language, target: (window as any).settings.target_language }))).toEqual({ source: 'en', target: 'fr' });
  await page.screenshot({ path: 'test-results/settings-languages.png' });
});

test('shortcut buttons each occupy one row; keys stay horizontal and recording commits on release', async ({ page }) => {
  await settingsFixture(page);
  const selection = page.getByRole('button', { name: 'Record selection shortcut', exact: true });
  const screenshot = page.getByRole('button', { name: 'Record screenshot shortcut', exact: true });
  for (const width of [1100, 850, 620]) {
    await page.setViewportSize({ width, height: 820 });
    const [a,b] = await Promise.all([selection.boundingBox(), screenshot.boundingBox()]);
    expect(a!.x).toBe(b!.x);
    expect(a!.width).toBe(b!.width);
    expect(a!.y + a!.height + 6).toBeLessThanOrEqual(b!.y);
    const keys = await selection.locator('kbd').evaluateAll(elements => elements.map(el => el.getBoundingClientRect().toJSON()));
    expect(keys.length).toBeGreaterThan(1);
    expect(keys[0].y).toBe(keys[1].y);
    expect(keys[0].right).toBeLessThan(keys[1].x);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  await selection.click();
  await page.keyboard.down('Alt');
  await page.keyboard.down('K');
  expect(await page.evaluate(() => (window as any).calls.filter((c:any) => c.cmd === 'end_shortcut_recording').length)).toBe(0);
  await page.keyboard.up('K');
  await page.keyboard.up('Alt');
  await expect(selection).toContainText('K');
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  expect(await page.evaluate(() => (window as any).settings.selection_shortcut)).toBe('Alt+K');
  await page.setViewportSize({ width: 1100, height: 820 });
  await page.screenshot({ path: 'test-results/shortcuts-light.png' });
  await page.getByRole('button', { name: 'Appearance', exact: true }).click();
  await page.getByRole('combobox', { name: 'Theme', exact: true }).selectOption('dark');
  await page.getByRole('button', { name: 'Translation', exact: true }).click();
  await page.screenshot({ path: 'test-results/shortcuts-dark.png' });
});

test('settings forms never render orphan chevron characters or redundant service chevrons', async ({ page }) => {
  await settingsFixture(page);
  for (const section of ['Services', 'Appearance']) {
    await page.getByRole('button', { name: section, exact: true }).click();
    const orphan = await page.locator('.pref-pane').evaluate(root => {
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      const found: string[] = []; let node;
      while (node = walker.nextNode()) if (node.textContent?.trim() === '>') found.push(node.parentElement!.tagName);
      return found;
    });
    expect(orphan).toEqual([]);
    if (section === 'Services') await expect(page.locator('.service-item > svg')).toHaveCount(0);
  }
});

test('quitting while settings load does not wait for a load with no user edits', async ({ page }) => {
  await settingsFixture(page,true);
  await page.evaluate(()=>(window as any).closeSettings('quit'));
  await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='finish_settings_close').length)).toBe(1);
  await expect(page.getByRole('dialog')).toBeHidden();
});

test('service types are always visible and reselecting custom never adds a duplicate', async ({ page }) => {
  await settingsFixture(page);
  await page.getByRole('button', { name: 'Services', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Add service', exact: true })).toHaveCount(0);
  await expect(page.locator('.service-list').getByRole('button', { name: /DeepSeek/ })).toBeVisible();
  await expect(page.locator('.service-list').getByRole('button', { name: /MiniMax/ })).toBeVisible();
  await expect(page.locator('.service-list').getByRole('button', { name: /Custom service/ })).toHaveCount(1);
  await page.locator('.service-list').getByRole('button', { name: /DeepSeek/ }).click();
  await page.locator('.service-list').getByRole('button', { name: /Custom service/ }).click();
  await page.locator('.service-list').getByRole('button', { name: /DeepSeek/ }).click();
  await expect(page.locator('.service-list').getByRole('button', { name: /DeepSeek/ })).toHaveCount(1);
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  expect(await page.evaluate(() => (window as any).settings.profiles.filter((p: any) => p.provider === 'custom').length)).toBe(1);
  await page.screenshot({ path: 'test-results/settings-services.png' });
});

test('close and quit prompt for unsaved settings; failures retain the dialog and draft', async ({ page }) => {
  await settingsFixture(page);
  await page.getByLabel('Default translation language', { exact: true }).selectOption('fr');
  await page.evaluate(() => (window as any).closeSettings());
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await page.getByRole('button', { name: 'Keep editing', exact: true }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByLabel('Default translation language', { exact: true })).toHaveValue('fr');
  await page.evaluate(() => { const w = window as any; w.failSave = true; w.closeSettings('quit'); });
  await page.getByRole('button', { name: 'Save and quit', exact: true }).click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('alert')).toHaveText('Fixture save failed');
  expect(await page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'finish_settings_close').length)).toBe(0);
  await page.evaluate(() => { (window as any).failSave = false; });
  await page.getByRole('button', { name: 'Save and quit', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'finish_settings_close').length)).toBe(1);
  expect(await page.evaluate(() => (window as any).settings.target_language)).toBe('fr');
});

test('history scroll areas contain their content and never overlap retention', async ({ page }) => {
  await settingsFixture(page);
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.locator('.history-row').first().click();
  await expect(page.getByLabel('Record details', { exact: true })).toContainText('第 0 条记录的译文');
  for (const width of [1100, 850, 620]) {
    await page.setViewportSize({ width, height: 820 });
    const bounds = await page.evaluate(() => {
      const workspace = document.querySelector('.history-workspace')!.getBoundingClientRect();
      const list = document.querySelector('.history-list')!.getBoundingClientRect();
      const detail = document.querySelector('.history-detail')!.getBoundingClientRect();
      const retention = document.querySelector('.retention-section')?.getBoundingClientRect();
      return { workspaceBottom: workspace.bottom, listBottom: list.bottom, detailBottom: detail.bottom, retentionTop: retention?.top, noHorizontalOverflow: document.documentElement.scrollWidth <= innerWidth };
    });
    expect(bounds.listBottom).toBeLessThanOrEqual(bounds.workspaceBottom + 1);
    expect(bounds.detailBottom).toBeLessThanOrEqual(bounds.workspaceBottom + 1);
    expect(bounds.retentionTop).toBeGreaterThanOrEqual(bounds.workspaceBottom);
    expect(bounds.noHorizontalOverflow).toBe(true);
  }
  await page.setViewportSize({ width: 1100, height: 820 });
  await page.locator('.settings-content').evaluate(el => el.scrollTop = 0);
  await page.screenshot({ path: 'test-results/history-light.png' });
  await page.locator('.history-list').evaluate(el => el.scrollTop = el.scrollHeight);
  await page.locator('.history-detail').evaluate(el => el.scrollTop = el.scrollHeight);
  await page.screenshot({ path: 'test-results/history-scroll-end.png' });
  await page.getByRole('button', { name: 'Appearance', exact: true }).click();
  await page.getByRole('combobox', { name: 'Theme', exact: true }).selectOption('dark');
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.locator('.history-row').first().click();
  await page.screenshot({ path: 'test-results/history-dark.png' });
});

test('clean close needs no prompt and discarding never saves the changed values', async ({ page }) => {
  await settingsFixture(page);
  await page.evaluate(()=>(window as any).closeSettings());
  await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='finish_settings_close').length)).toBe(1);
  await expect(page.getByRole('dialog')).toBeHidden();
  await page.getByLabel('Default translation language',{exact:true}).selectOption('fr');
  await page.evaluate(()=>(window as any).closeSettings());
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.getByRole('button',{name:'Don’t save',exact:true}).click();
  await expect(page.getByRole('dialog')).toBeHidden();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='save_configuration').length)).toBe(0);
  await expect(page.getByLabel('Default translation language',{exact:true})).toHaveValue('zh-Hans');
});

test('filtering invalidates a pending detail so old content cannot return', async ({ page }) => {
  await settingsFixture(page);
  await page.getByRole('button',{name:'History',exact:true}).click();
  await page.evaluate(()=>(window as any).delayDetail=true);
  await page.locator('.history-row').first().click();
  await expect.poll(()=>page.evaluate(()=>typeof (window as any).releaseDetail)).toBe('function');
  await page.getByRole('searchbox').fill('no match');
  await page.getByRole('button',{name:'Search',exact:true}).click();
  await page.evaluate(()=>(window as any).releaseDetail());
  await expect(page.getByLabel('Record details',{exact:true})).toContainText('Choose a record');
  await expect(page.getByLabel('Record details',{exact:true})).not.toContainText('Record 0:');
});


test('permission badges refresh on return and while awaiting authorization, without replacing drafts', async ({ page }) => {
  await settingsFixture(page);
  await page.getByLabel('Default translation language', {exact:true}).selectOption('fr');
  await page.evaluate(() => { const w=window as any; w.permissions={accessibility:false,screen:false}; window.dispatchEvent(new Event('focus')); });
  await expect(page.locator('.permission-state')).toHaveText(['Not allowed','Not allowed']);
  await page.evaluate(() => { const w=window as any; w.permissions.accessibility=true; window.dispatchEvent(new Event('focus')); });
  await expect(page.locator('.permission-state')).toHaveText(['Allowed','Not allowed']);
  await page.evaluate(() => { (window as any).permissions.screen=true; });
  await expect(page.locator('.permission-state')).toHaveText(['Allowed','Allowed']);
  await expect(page.getByLabel('Default translation language', {exact:true})).toHaveValue('fr');
  const button=page.getByRole('button',{name:'Refresh status',exact:true});
  const box=await button.boundingBox();
  const text=await button.evaluate(el=>{const range=document.createRange();range.selectNodeContents(el);return range.getBoundingClientRect().toJSON();});
  expect(text.left-box!.x).toBeGreaterThanOrEqual(10);
  expect(box!.x+box!.width-text.right).toBeGreaterThanOrEqual(10);
  await page.locator('.permission-core').screenshot({path:'test-results/permission-card.png'});
});

test('shortcut hint follows the action label and aligns at its lower edge', async ({ page }) => {
  await settingsFixture(page);
  const button=page.getByRole('button',{name:'Record selection shortcut',exact:true});
  const operation=await button.locator('.shortcut-operation').boundingBox();
  const hint=await button.locator('.recorder-action').boundingBox();
  const caption=await button.locator('.shortcut-label').boundingBox();
  const caps=await button.locator('.shortcut-keycaps').boundingBox();
  expect(hint!.x).toBeLessThan(caps!.x);
  expect(hint!.x).toBeLessThan(operation!.x+operation!.width);
  expect(Math.abs(hint!.y+hint!.height-(caption!.y+caption!.height))).toBeLessThanOrEqual(4);
});

test('history controls share a baseline and statuses are tags with concise Chinese labels', async ({page}) => {
  await settingsFixture(page);
  await page.evaluate(()=>{const w=window as any;w.records[1].status='failed';});
  await page.getByRole('button',{name:'History',exact:true}).click();
  for(const width of [1100,850]){
    await page.setViewportSize({width,height:820});
    const boxes=await page.locator('.history-search-field,.history-tools select,.history-search-button').evaluateAll(elements=>elements.map(el=>el.getBoundingClientRect().toJSON()));
    expect(new Set(boxes.map(box=>Math.round(box.top))).size).toBe(1);
    expect(new Set(boxes.map(box=>Math.round(box.height))).size).toBe(1);
  }
  await expect(page.locator('.history-status.complete').first()).not.toHaveCSS('background-color','rgba(0, 0, 0, 0)');
  await page.getByRole('button',{name:'Appearance',exact:true}).click();
  await page.getByRole('combobox',{name:'Display language',exact:true}).selectOption('zh');
  await page.getByRole('button',{name:'历史记录',exact:true}).click();
  await expect(page.locator('.history-status.complete').first()).toHaveText('完成');
  await expect(page.locator('.history-status.failed').first()).toHaveText('失败');
  await page.screenshot({path:'test-results/history-aligned-zh.png'});
});


test('service catalog configures models, a local dictionary and separate Youdao credentials', async ({page}) => {
  await settingsFixture(page);
  await page.getByRole('button',{name:'Services',exact:true}).click();
  for(const name of ['Gemini','OpenAI','Claude','Qwen','Zhipu','SiliconFlow','OpenRouter','System dictionary','Youdao'])await expect(page.locator('.service-list').getByRole('button',{name:new RegExp(name)})).toHaveCount(1);
  await page.locator('.service-list').getByRole('button',{name:/Gemini/}).click();
  await page.getByRole('combobox',{name:'Model',exact:true}).fill('fixture-gemini');
  await page.getByLabel('API Key',{exact:true}).fill('fixture-only-key');
  await page.locator('.service-list').getByRole('button',{name:/System dictionary/}).click();
  await expect(page.getByLabel('API Key',{exact:true})).toHaveCount(0);
  await expect(page.getByRole('button',{name:'Test dictionary',exact:true})).toBeVisible();
  await expect(page.locator('.service-list').getByRole('button',{name:/Custom service/}).locator('small')).toHaveText('Configured');
  await expect(page.locator('.service-list').getByRole('button',{name:/Custom service/}).locator('.service-default-tag')).toHaveText('Default');
  await page.getByRole('button',{name:'Use by default',exact:true}).click();
  await expect(page.locator('.service-default-tag')).toHaveCount(1);
  await expect(page.locator('.service-list').getByRole('button',{name:/System dictionary/}).locator('.service-default-tag')).toHaveText('Default');
  await expect(page.locator('.service-list').getByRole('button',{name:/System dictionary/}).locator('small')).toHaveText('On this Mac');
  await expect(page.locator('.service-list').getByRole('button',{name:/Custom service/}).locator('small')).toHaveText('Configured');
  await expect(page.getByRole('button',{name:'Fetch models',exact:true})).toHaveCount(0);
  await page.locator('.service-list').getByRole('button',{name:/Youdao/}).click();
  await page.getByLabel('Application ID',{exact:true}).fill('fixture-app');
  await page.getByLabel('Application secret',{exact:true}).fill('fixture-only-secret');
  await expect(page.getByRole('combobox',{name:'Model',exact:true})).toHaveCount(0);
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  const saved=await page.evaluate(()=>{const w=window as any;return {providers:w.settings.profiles.map((p:any)=>p.provider),id:w.settings.profiles.find((p:any)=>p.provider==='youdao').app_id,keyUpdates:w.calls.filter((c:any)=>c.cmd==='save_configuration').at(-1).args.apiKeyUpdates};});
  expect(saved.providers).toEqual(['custom','gemini','system_dictionary','youdao']);expect(saved.id).toBe('fixture-app');expect(Object.values(saved.keyUpdates)).toContain('fixture-only-secret');
  await page.locator('.service-list').getByRole('button',{name:/Gemini/}).click();
  await expect(page.locator('.service-list').getByRole('button',{name:/Gemini/})).toHaveCount(1);
  await page.screenshot({path:'test-results/services-catalog.png'});
  await page.getByRole('button',{name:'Appearance',exact:true}).click();
  await page.getByRole('combobox',{name:'Display language',exact:true}).selectOption('zh');
  await page.getByRole('button',{name:'服务',exact:true}).click();
  await page.getByRole('button',{name:'保存设置',exact:true}).click();
  await page.screenshot({path:'test-results/services-catalog-zh.png'});
});

test('service search supports Chinese English and pinyin; panes align and prompt saves',async({page})=>{
 await settingsFixture(page);await page.getByRole('button',{name:'Services',exact:true}).click();
 await expect(page.getByText('Connect your own model services. Keys stay in the system keychain.',{exact:true})).toHaveCount(0);
 const search=page.getByRole('textbox',{name:'Search providers'});
 for(const query of ['智谱','zhipu','zp']){await search.fill(query);await expect(page.locator('.service-scroll .service-item')).toHaveCount(1);await expect(page.locator('.service-scroll')).toContainText('Zhipu')}
 await page.locator('.service-scroll .service-item').click();
 await expect(page.getByLabel('Model',{exact:true})).toHaveValue('glm-4.7-flash');
 await search.fill('');
 const panes=await page.evaluate(()=>['.service-list','.service-editor-tray'].map(q=>document.querySelector(q)!.getBoundingClientRect().toJSON()));expect(panes[0].height).toBe(panes[1].height);expect(panes[0].y).toBe(panes[1].y);
 await expect(page.locator('.provider-logo')).toHaveCount(13);
 await page.getByLabel('Translation prompt source',{exact:true}).selectOption('custom');
 const prompt=page.getByRole('textbox',{name:'Service translation prompt',exact:true});await expect(prompt).toHaveValue(/---VOCAB_JSON---/);
 await prompt.fill('Translate into {{target_lang}} with concise technical wording.');
 await page.getByRole('button',{name:'Save changes',exact:true}).click();
 expect(await page.evaluate(()=>(window as any).settings.profiles.find((p:any)=>p.provider==='zhipu').custom_prompt)).toContain('{{target_lang}}');
 await page.getByLabel('Translation prompt source',{exact:true}).selectOption('global');await expect(prompt).toHaveCount(0);
 await page.screenshot({path:'artifacts/previews/2026-10-15-services-prompts.png'});
});

test('provider logos load actual SVG artwork',async({page})=>{await settingsFixture(page);await page.getByRole('button',{name:'Services',exact:true}).click();const logo=page.locator('.brand-silhouette').first();await expect(logo.locator('svg path').first()).toHaveAttribute('d',/.+/);});


test('vocabulary searches contexts, saves notes, manages mastery and exports all formats',async({page})=>{
 await settingsFixture(page);await page.getByRole('button',{name:'Vocabulary',exact:true}).click();
 await page.getByRole('searchbox',{name:'Search vocabulary'}).fill('adapts');await expect(page.locator('.vocabulary-item')).toHaveCount(1);
 await page.locator('.vocabulary-item').click();await expect(page.locator('.context-source mark')).toHaveText('resilient');
 await page.getByRole('textbox',{name:'Personal note'}).fill('Recover after a difficult change.');
 await expect.poll(()=>page.evaluate(()=>(window as any).vocabulary[0].note)).toBe('Recover after a difficult change.');
 await page.getByRole('button',{name:'Mark as mastered'}).click();await expect(page.getByRole('button',{name:'Mark as learning'})).toBeVisible();
 await page.getByLabel('Learning status',{exact:true}).selectOption('learning');await expect(page.locator('.vocabulary-item')).toHaveCount(0);await page.getByLabel('Learning status',{exact:true}).selectOption('all');
 await page.getByRole('button',{name:'Pronounce word'}).click();expect(await page.evaluate(()=>(window as any).calls.some((c:any)=>c.cmd==='speak_word'&&c.args.word==='resilient'))).toBe(true);
 await expect(page.getByLabel('Export format',{exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'Export',exact:true}).click();await page.keyboard.press('Escape');await expect(page.getByRole('dialog',{name:'Export vocabulary'})).not.toBeVisible();
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='vocabulary_export').length)).toBe(0);
 for(const [format,label] of [['csv','CSV Spreadsheets'],['markdown','Markdown Notes and documents'],['anki','Anki TSV Flashcards']]){
  await page.getByRole('button',{name:'Export',exact:true}).click();await page.getByRole('dialog',{name:'Export vocabulary'}).getByRole('button',{name:label,exact:true}).click();
  await expect(page.getByRole('status').filter({hasText:'Vocabulary exported'})).toBeVisible();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='vocabulary_export').at(-1).args.format)).toBe(format);
 }
 const panes=await page.evaluate(()=>['.vocabulary-list-tray','.vocabulary-detail-tray'].map(q=>document.querySelector(q)!.getBoundingClientRect().toJSON()));expect(panes[0].height).toBe(panes[1].height);
 await page.screenshot({path:'artifacts/previews/2026-10-15-vocabulary.png'});
 await page.getByRole('button',{name:'Delete context',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'Delete',exact:true}).click();await expect(page.locator('.word-context')).toHaveCount(0);await page.getByRole('searchbox',{name:'Search vocabulary'}).fill('');
 await page.getByRole('button',{name:'Delete word',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'Cancel',exact:true}).click();await expect(page.locator('.vocabulary-item')).toHaveCount(1);
 await page.getByRole('button',{name:'Delete word',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'Delete',exact:true}).click();await expect(page.locator('.vocabulary-item')).toHaveCount(0);
});

test('service and vocabulary workspaces remain usable in narrow and dark windows',async({page})=>{
 await settingsFixture(page);
 for(const section of ['Services','Vocabulary']){
  await page.getByRole('button',{name:section,exact:true}).click();if(section==='Vocabulary')await page.locator('.vocabulary-item').click();
  for(const width of [1100,850,620]){await page.setViewportSize({width,height:820});expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);}
 }
 await page.setViewportSize({width:1100,height:820});await page.evaluate(()=>document.documentElement.setAttribute('data-theme','dark'));
 await page.screenshot({path:'artifacts/previews/2026-10-15-vocabulary-dark.png'});
});


test('free defaults, tier labels and live OpenRouter filtering retain exact model IDs',async({page})=>{
 await settingsFixture(page);await page.getByRole('button',{name:'Services',exact:true}).click();
 for(const [provider,model] of [['Gemini','gemini-2.5-flash-lite'],['SiliconFlow','Qwen/Qwen2.5-7B-Instruct'],['Zhipu','glm-4.7-flash']]){
  await page.locator('.service-list').getByRole('button',{name:new RegExp(provider)}).click();await expect(page.getByLabel('Model',{exact:true})).toHaveValue(model);
 }
 await expect(page.locator('.model-suggestions')).toContainText('glm-4.7-flash');await expect(page.locator('.model-suggestions')).toContainText('glm-4.6v-flash');
 await page.locator('.service-list').getByRole('button',{name:/Gemini/}).click();await expect(page.locator('.model-catalog')).toContainText('Free tier');
 await page.getByLabel('Model',{exact:true}).fill('my-existing-model');await page.locator('.service-list').getByRole('button',{name:/SiliconFlow/}).click();await page.locator('.service-list').getByRole('button',{name:/Gemini/}).click();await expect(page.getByLabel('Model',{exact:true})).toHaveValue('my-existing-model');
 await page.locator('.service-list').getByRole('button',{name:/OpenRouter/}).click();await page.getByRole('button',{name:'Fetch models',exact:true}).click();
 const list=page.locator('.model-suggestions');await expect(list.getByRole('button')).toHaveCount(1);await expect(list).toContainText('fixture/free-model:free');
 await list.getByRole('button').click();await expect(page.getByLabel('Model',{exact:true})).toHaveValue('fixture/free-model:free');
 await page.getByLabel('Show free models only',{exact:true}).uncheck();await expect(list.getByRole('button')).toHaveCount(3);await expect(list.getByRole('button',{name:/zero-input/})).toContainText('Paid');
 await expect(page.getByLabel('Model',{exact:true})).toHaveValue('fixture/free-model:free');
 await page.getByRole('button',{name:'Save changes',exact:true}).click();expect(await page.evaluate(()=>(window as any).settings.profiles.find((p:any)=>p.provider==='openrouter').model)).toBe('fixture/free-model:free');
 await page.screenshot({path:'artifacts/previews/2026-10-15-free-model-catalog.png'});
});

test('shared translation and reply prompts save independently and services inherit or override them',async({page})=>{
 await settingsFixture(page);await page.getByRole('button',{name:'Prompts',exact:true}).click();
 const translation=page.getByRole('textbox',{name:'Translation prompt',exact:true});await expect(translation).toHaveValue(/Mode: sentence translation/);
 await expect(translation).toHaveAttribute('aria-describedby','global-prompt-help');
 const help=page.locator('.global-prompts .prompt-help');await expect(help).toHaveCount(1);
 await expect(help).toContainText('override per service');
 expect((await help.boundingBox())!.y).toBeGreaterThanOrEqual((await translation.boundingBox())!.y+(await translation.boundingBox())!.height);
 await expect(page.locator('.global-prompts > p.subtle:not([role])')).toHaveCount(0);

 await page.screenshot({path:'artifacts/previews/reading-workflow-prompts-simple.png'});
 await expect(page.getByRole('button',{name:'Save changes',exact:true})).toBeDisabled();
 await translation.fill('Keep domain terms precise.');await page.getByRole('button',{name:'Save changes',exact:true}).click();expect(await page.evaluate(()=>(window as any).settings.translation_prompt)).toBe('Keep domain terms precise.');
 await page.getByRole('button',{name:'Reply writing',exact:true}).click();const reply=page.getByRole('textbox',{name:'Reply prompt',exact:true});await expect(reply).toHaveValue(/native reader/);await reply.fill('Polish the intended reply without inventing facts.');await page.getByRole('button',{name:'Save changes',exact:true}).click();
 expect(await page.evaluate(()=>(window as any).settings.reply_prompt)).toBe('Polish the intended reply without inventing facts.');expect(await page.evaluate(()=>(window as any).settings.profiles[0].custom_prompt??'')).toBe('');
 await page.getByRole('button',{name:'Services',exact:true}).click();await expect(page.getByLabel('Translation prompt source')).toHaveValue('global');await expect(page.getByLabel('Reply prompt source')).toHaveValue('global');await expect(page.getByRole('textbox',{name:'Service translation prompt'})).toHaveCount(0);
 await page.getByLabel('Reply prompt source').selectOption('custom');const custom=page.getByRole('textbox',{name:'Service reply prompt'});await expect(custom).toHaveValue('Polish the intended reply without inventing facts.');await custom.fill('This service uses a concise reply.');await page.getByRole('button',{name:'Save changes',exact:true}).click();expect(await page.evaluate(()=>(window as any).settings.profiles[0].custom_reply_prompt)).toBe('This service uses a concise reply.');
 await page.getByLabel('Reply prompt source').selectOption('global');await page.getByRole('button',{name:'Save changes',exact:true}).click();expect(await page.evaluate(()=>(window as any).settings.profiles[0].custom_reply_prompt)).toBe('');
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>['submit_operation','test_connection','translate_text','retry_task'].includes(c.cmd)))).toEqual([]);
});

for (const kind of ['Translation', 'Reply'] as const) {
  test(`${kind} prompt source follows saved and reverted settings without closing an empty draft`, async ({ page }) => {
    await settingsFixture(page, false, 'Custom service', true);
    await page.getByRole('button', { name: 'Services', exact: true }).click();
    const selector = page.getByLabel(`${kind} prompt source`, { exact: true });
    const editor = page.getByRole('textbox', { name: `Service ${kind.toLowerCase()} prompt`, exact: true });
    const save = page.getByRole('button', { name: 'Save changes', exact: true });
    const revert = page.getByRole('button', { name: 'Revert', exact: true });
    const prompt = `Synthetic saved custom ${kind.toLowerCase()} instructions.`;
    await selector.selectOption('custom');
    await editor.fill(prompt);
    await save.click();
    await expect(selector).toHaveValue('custom');
    await selector.selectOption('global');
    await revert.click();
    await expect(selector).toHaveValue('custom');
    await expect(editor).toHaveValue(prompt);

    await page.locator('.service-list').getByRole('button', { name: /Backup service/ }).click();
    await expect(selector).toHaveValue('global');
    await page.locator('.service-list').getByRole('button', { name: /Custom service/ }).click();
    await expect(selector).toHaveValue('custom');
    await expect(editor).toHaveValue(prompt);
    await editor.fill('');
    await expect(selector).toHaveValue('custom');
    await expect(editor).toBeVisible();
    await editor.fill('Synthetic replacement instructions.');
    await expect(editor).toHaveValue('Synthetic replacement instructions.');
    await editor.fill('');
    await save.click();
    await expect(selector).toHaveValue('global');
    await expect(editor).toHaveCount(0);

    await selector.selectOption('custom');
    await editor.fill('Unsaved custom instructions.');
    await revert.click();
    await expect(selector).toHaveValue('global');
    await expect(editor).toHaveCount(0);
  });
}


test('automatic vocabulary switch saves and pure-translation preview has no metadata contract',async({page})=>{
 await settingsFixture(page);const toggle=page.getByRole('checkbox',{name:/Automatically extract vocabulary/});await expect(toggle).toBeChecked();await toggle.uncheck();await page.getByRole('button',{name:'Save changes',exact:true}).click();expect(await page.evaluate(()=>(window as any).settings.auto_vocabulary)).toBe(false);
 await page.getByRole('button',{name:'Prompts',exact:true}).click();const prompt=page.getByRole('textbox',{name:'Translation prompt',exact:true});await expect(prompt).not.toHaveValue(/---VOCAB_JSON---|Extract 2|"meaning"/);await expect(prompt).toHaveValue(/Return only/);
});


test('compact settings keep workspace bottoms near footer at different window sizes',async({page})=>{
 await settingsFixture(page);
 await expect(page.getByText('Choose your target language and how you capture text.')).toHaveCount(0);
 await expect(page.getByText('Skipped above 150 words or 800 characters')).toHaveCount(0);
 await expect(page.getByRole('checkbox',{name:'Local sensitive-data protection',exact:true})).toBeChecked();
 for(const [width,height] of [[1100,820],[850,650],[1400,1100]]){
  await page.setViewportSize({width,height});
  for(const [section,workspace] of [['Services','.service-workspace'],['Vocabulary','.vocabulary-workspace']]){
   await page.getByRole('button',{name:section,exact:true}).click();
   const result=await page.evaluate((selector)=>{
    const box=document.querySelector(selector)!.getBoundingClientRect();
    const footer=document.querySelector('.settings-actions')!.getBoundingClientRect();
    return {gap:footer.top-box.bottom,height:box.height,overflow:document.documentElement.scrollWidth>innerWidth};
   },workspace);
   expect(result.gap).toBeGreaterThanOrEqual(8);expect(result.gap).toBeLessThanOrEqual(16);expect(result.height).toBeGreaterThan(250);expect(result.overflow).toBe(false);
  }
 }
 await page.setViewportSize({width:1100,height:820});
 await expect(page.getByText('Build your vocabulary through words and sentences, with every context in one place.')).toHaveCount(0);
 const controls=await page.locator('.vocabulary-toolbar').evaluate(el=>Array.from(el.children).map(c=>c.getBoundingClientRect().height));
 expect(Math.max(...controls)-Math.min(...controls)).toBeLessThanOrEqual(1);
 await page.screenshot({path:'artifacts/previews/2026-10-15-compact-vocabulary-toolbar.png'});
 await page.getByRole('button',{name:'Export',exact:true}).click();
 await page.screenshot({path:'artifacts/previews/2026-10-15-export-format-dialog.png'});
});


test('history uses the same Markdown reader and plain text copy without requesting translation',async({page})=>{
 await settingsFixture(page);await page.getByRole('button',{name:'History',exact:true}).click();
 await page.evaluate(()=>{const w=window as any;w.records[0].translation='# History result\n\n- **Resilient** team\n- Second line';});
 await page.locator('.history-row').first().click();await expect(page.locator('.history-detail .markdown-reading h1')).toHaveText('History result');
 await page.getByRole('button',{name:'Plain text',exact:true}).click();await expect(page.locator('.history-detail .translation-prose')).toContainText('**Resilient**');
 await page.getByRole('button',{name:'Copy translation',exact:true}).click();expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='copy_text').at(-1).args.text)).toContain('• Resilient team');
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='retry_task').length)).toBe(0);
});

test('default service moves first and personal names survive display localization', async ({page}) => {
  await settingsFixture(page, false, '自定义模型');
  await page.getByRole('button',{name:'Services',exact:true}).click();
  await expect(page.locator('.service-item').first()).toContainText('Custom service');
  await page.locator('.service-item').filter({hasText:'Zhipu'}).click();
  await page.getByLabel('API Key',{exact:true}).fill('fixture-only-key');
  await page.getByRole('button',{name:'Use by default',exact:true}).click();
  await expect(page.locator('.service-item').first()).toContainText('Zhipu');
  await expect(page.locator('.service-item').last()).toContainText('Custom service');
  await page.locator('.service-item').last().click();
  await expect(page.getByLabel('Service name',{exact:true})).toHaveValue('Custom service');
  await page.getByLabel('Service name',{exact:true}).fill('我的代理');
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await expect(page.getByLabel('Service name',{exact:true})).toHaveValue('我的代理');
});

test('native entrance animations respect changes to reduced motion', async ({page}) => {
  await settingsFixture(page);
  await page.emulateMedia({reducedMotion:'no-preference'});
  await page.getByRole('button',{name:'Services',exact:true}).click();
  await page.getByRole('button',{name:'Translation',exact:true}).click();
  await expect.poll(() => page.evaluate(() => document.getAnimations().length)).toBeGreaterThan(0);
  await page.emulateMedia({reducedMotion:'reduce'});
  await expect.poll(() => page.evaluate(() => document.getAnimations().length)).toBe(0);
  await expect(page.getByLabel('Default translation language',{exact:true})).toBeVisible();
});


test('permission recovery explains re-adding Yurelay in order and trusts only refreshed permission status',async({page})=>{
  await settingsFixture(page);
  const recovery=page.locator('details.permission-recovery');
  await expect(recovery).toHaveCount(0);
  const target=page.getByLabel('Default translation language',{exact:true});
  await target.selectOption('fr');
  await page.evaluate(()=>{const w=window as any;w.permissions={accessibility:false,screen:false};window.dispatchEvent(new Event('focus'));});
  await expect(page.locator('.permission-state')).toHaveText(['Not allowed','Not allowed']);
  await expect(recovery).toBeVisible();
  await expect.poll(()=>recovery.evaluate((el:HTMLDetailsElement)=>el.open)).toBe(false);
  const summary=recovery.locator('summary');
  await expect(summary).toHaveText('Updated Yurelay but still not allowed?');
  await summary.click();
  const steps=recovery.locator('ol > li');
  await expect(steps).toHaveCount(4);
  await expect(steps.nth(0)).toContainText(/remove/i);
  await expect(steps.nth(0)).toContainText('Yurelay');
  await expect(steps.nth(0)).toContainText('−');
  await expect(steps.nth(1)).toContainText(/add/i);
  await expect(steps.nth(1)).toContainText('Yurelay');
  await expect(steps.nth(1)).toContainText(/[+＋]/);
  await expect(steps.nth(2)).toContainText(/enable|turn on/i);
  await expect(steps.nth(2)).toContainText(/Quit.*Reopen/i);
  await expect(steps.nth(3)).toContainText(/Refresh/i);
  await page.locator('.permission-core').screenshot({path:'artifacts/previews/2026-10-16-permission-recovery-en.png'});
  await summary.click();
  await expect.poll(()=>recovery.evaluate((el:HTMLDetailsElement)=>el.open)).toBe(false);
  for(const [index,kind] of [[0,'accessibility'],[1,'screen']] as const){
    await page.locator('.permission-row').nth(index).getByRole('button',{name:'Open system settings',exact:true}).click();
    await expect.poll(()=>recovery.evaluate((el:HTMLDetailsElement)=>el.open)).toBe(true);
    expect(await page.evaluate(()=>(window as any).calls.filter((call:any)=>call.cmd==='request_permission').at(-1).args)).toEqual({kind});
    await expect(page.locator('.permission-state')).toHaveText(['Not allowed','Not allowed']);
    await expect(target).toHaveValue('fr');
  }
  await page.evaluate(()=>{const w=window as any;w.permissions.accessibility=true;window.dispatchEvent(new Event('focus'));});
  await expect(page.locator('.permission-state')).toHaveText(['Allowed','Not allowed']);
  await expect(recovery).toBeVisible();
  const previousReads=await page.evaluate(()=>(window as any).calls.filter((call:any)=>call.cmd==='permissions').length);
  await page.evaluate(()=>{(window as any).permissions.screen=true;});
  await page.getByRole('button',{name:'Refresh status',exact:true}).click();
  await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((call:any)=>call.cmd==='permissions').length)).toBeGreaterThan(previousReads);
  await expect(page.locator('.permission-state')).toHaveText(['Allowed','Allowed']);
  await expect(recovery).toHaveCount(0);
  await expect(target).toHaveValue('fr');
  await expect(page.getByRole('button',{name:'Save changes',exact:true})).toBeEnabled();
  expect(await page.evaluate(()=>(window as any).settings.target_language)).toBe('zh-Hans');
  expect(await page.evaluate(()=>(window as any).calls.filter((call:any)=>call.cmd==='save_configuration'))).toEqual([]);
});

test('permission recovery localizes its collapsed guide and ordered actions in Chinese',async({page})=>{
  await settingsFixture(page);
  await page.evaluate(()=>{const w=window as any;w.permissions={accessibility:false,screen:true};window.dispatchEvent(new Event('focus'));});
  await expect(page.locator('.permission-state')).toHaveText(['Not allowed','Allowed']);
  await page.getByRole('button',{name:'Appearance',exact:true}).click();
  await page.getByRole('combobox',{name:'Display language',exact:true}).selectOption('zh');
  await page.getByRole('button',{name:'翻译与快捷键',exact:true}).click();
  const recovery=page.locator('details.permission-recovery');
  await expect(recovery).toBeVisible();
  await expect.poll(()=>recovery.evaluate((el:HTMLDetailsElement)=>el.open)).toBe(false);
  await expect(recovery.locator('summary')).toHaveText('更新后仍未授权？');
  await recovery.locator('summary').click();
  const steps=recovery.locator('ol > li');
  await expect(steps).toHaveCount(4);
  await expect(steps.nth(0)).toContainText(/移除|删除/);
  await expect(steps.nth(0)).toContainText('−');
  await expect(steps.nth(1)).toContainText(/添加|加入/);
  await expect(steps.nth(1)).toContainText(/[+＋]/);
  await expect(steps.nth(2)).toContainText(/启用|开启|打开/);
  await expect(steps.nth(2)).toContainText(/退出.*重新|Quit.*Reopen/);
  await expect(steps.nth(3)).toContainText('刷新');
  await page.locator('.permission-core').screenshot({path:'artifacts/previews/2026-10-16-permission-recovery-zh.png'});
  expect(await page.evaluate(()=>(window as any).calls.filter((call:any)=>call.cmd==='request_permission'))).toEqual([]);
});


test('permission recovery stays readable in narrow and dark settings without covering the footer',async({page})=>{
  await settingsFixture(page);
  await page.evaluate(()=>{const w=window as any;w.permissions={accessibility:false,screen:false};window.dispatchEvent(new Event('focus'));});
  await expect(page.locator('.permission-state')).toHaveText(['Not allowed','Not allowed']);
  for(const theme of ['light','dark']){
    await page.getByRole('button',{name:'Appearance',exact:true}).click();
    await page.getByRole('combobox',{name:'Theme',exact:true}).selectOption(theme);
    await page.getByRole('button',{name:'Translation',exact:true}).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme',theme);
    const recovery=page.locator('details.permission-recovery');
    if(!await recovery.evaluate((el:HTMLDetailsElement)=>el.open))await recovery.locator('summary').click();
    for(const width of [1100,620]){
      await page.setViewportSize({width,height:820});
      await page.locator('.settings-content').evaluate(el=>el.scrollTop=el.scrollHeight);
      await expect(recovery.locator('ol > li').last()).toBeInViewport();
      await expect(page.getByRole('button',{name:'Refresh status',exact:true})).toBeInViewport();
      await expect(page.getByRole('button',{name:'Save changes',exact:true})).toBeInViewport();
      const layout=await page.evaluate(()=>{
        const guide=document.querySelector('details.permission-recovery')!,content=document.querySelector('.settings-content')!,footer=document.querySelector('.settings-actions')!,refresh=document.querySelector('.permission-refresh')!;
        return {overflow:document.documentElement.scrollWidth>innerWidth,guideOverflow:guide.scrollWidth>guide.clientWidth,contentBottom:content.getBoundingClientRect().bottom,footerTop:footer.getBoundingClientRect().top,footerBottom:footer.getBoundingClientRect().bottom,refreshBottom:refresh.getBoundingClientRect().bottom,height:innerHeight};
      });
      expect(layout.overflow).toBe(false);expect(layout.guideOverflow).toBe(false);
      expect(layout.contentBottom).toBeLessThanOrEqual(layout.footerTop);
      expect(layout.refreshBottom).toBeLessThanOrEqual(layout.footerTop);
      expect(layout.footerBottom).toBeLessThanOrEqual(layout.height);
      await page.screenshot({path:`artifacts/previews/2026-10-16-permission-recovery-${width}-${theme}.png`});
    }
  }
});

test('one fallback switch uses configured services and saves with optional append shortcuts without probing',async({page})=>{
 await settingsFixture(page,false,'Custom service',true);
 await page.getByRole('checkbox',{name:/Automatic fallback services/}).check();
 await expect(page.locator('.failover-choices,.failover-list')).toHaveCount(0);
 await expect(page.getByRole('checkbox',{name:/Automatically tidy PDF line breaks/})).toBeChecked();
 await page.screenshot({path:'artifacts/previews/reading-workflow-settings-simple.png'});
 const order=await page.locator('.guard-setting input').evaluateAll(nodes=>nodes.map(node=>node.parentElement?.textContent??''));expect(order[0]).toContain('Automatic fallback services');expect(order[1]).toContain('Automatically extract vocabulary');
 await page.getByText('Optional append shortcuts',{exact:true}).click();await recordAppendShortcut(page,'selection','D');await recordAppendShortcut(page,'screenshot','S');
 expect(await page.evaluate(()=>(window as any).settings.failover)).toEqual({enabled:false,profile_ids:[]});
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>['test_connection','retry_task','submit_operation','translate_text'].includes(c.cmd)))).toEqual([]);
 await page.getByRole('button',{name:'Save changes',exact:true}).click();
 expect(await page.evaluate(()=>(window as any).settings.failover)).toEqual({enabled:true,profile_ids:[]});expect(await page.evaluate(()=>(window as any).settings.active_profile)).toBe('default');
 expect(await page.evaluate(()=>(window as any).settings.append_selection_shortcut)).toBe('Alt+Shift+D');
});

test('service receipts identify saved configuration and never mark a changed draft as verified',async({page})=>{
 await settingsFixture(page);await page.evaluate(()=>{const w=window as any;w.receiptOnTest=true;w.invalidateVerificationsOnSave=true;w.verifications=[{profile_id:'default',state:'verified',tested_at:Date.now(),expires_at:Date.now()+86400000,tested_capability:'translation_transport'}];});
 await page.getByRole('button',{name:'Services',exact:true}).click();await expect(page.locator('.service-editor .verification-state')).toContainText('passed a translation transport test');
 await page.getByLabel('Model',{exact:true}).fill('changed-model');await expect(page.locator('.service-editor .verification-state')).toContainText('unsaved changes');
 await page.getByRole('button',{name:'Test connection',exact:true}).click();await expect(page.locator('.service-editor .verification-state')).toContainText('do not apply to this draft');expect(await page.evaluate(()=>(window as any).settings.profiles[0].model)).toBe('fixture-model');
 await page.getByRole('button',{name:'Save changes',exact:true}).click();await expect(page.locator('.service-editor .verification-state')).toContainText('unverified');
 await page.getByRole('button',{name:'Test connection',exact:true}).click();await expect(page.locator('.service-editor .verification-state')).toContainText('passed a translation transport test');
});

test('standalone reply history is readable and copyable without attaching it to a draft or releasing sensitive operations',async({page})=>{
 await settingsFixture(page);await page.evaluate(()=>{const w=window as any;const base={request_id:'saved-request',task_id:null,source_revision:0,kind:'reply',status:'completed',primary_text:'A safely saved standalone reply.',back_translation:'已保存的独立回复。',back_translation_state:'complete',profile:w.settings.profiles[0],attempt:1,attempts:[],error:null,sensitive:null,output_language:'en',reference_language:'zh-Hans',created_at:1700000000000,updated_at:1700000000000,revision:1};w.operations=[{...base,id:'standalone'},{...base,id:'blocked',status:'blocked',primary_text:'',back_translation:'',sensitive:'API_SECRET'},{...base,id:'associated',task_id:'record-0',primary_text:'Associated result.'}];});
 await page.getByRole('button',{name:'History',exact:true}).click();expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='operation_list'))).toEqual([]);
 await page.getByText('Explanation and reply history',{exact:true}).click();await expect(page.locator('.operation-history-record')).toHaveCount(3);await page.locator('.operation-history-record>summary').first().click();await expect(page.locator('.operation-history')).toContainText('A safely saved standalone reply.');
 await page.locator('.operation-history-record').first().getByRole('button',{name:'Copy reply',exact:true}).click();expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='copy_text').at(-1).args.text)).toBe('A safely saved standalone reply.');
 await page.locator('.operation-history-record>summary').nth(1).click();await expect(page.locator('.operation-history-record').nth(1)).toContainText('original draft');await expect(page.locator('.operation-history').getByRole('button',{name:'Allow this request'})).toHaveCount(0);
 await page.locator('.operation-history-record>summary').nth(2).click();await page.getByRole('button',{name:'View associated source'}).click();await expect(page.locator('.history-detail')).toContainText('Record 0: A complete source');
 await page.getByRole('button',{name:'Delete record',exact:true}).click();await page.getByRole('button',{name:'Delete permanently',exact:true}).click();await expect(page.locator('.operation-history-record')).toHaveCount(2);await expect(page.locator('.operation-history')).not.toContainText('Associated result.');
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>['submit_operation','retry_task','translate_text','resolve_operation_sensitive'].includes(c.cmd)))).toEqual([]);
});


test('settings deep links preserve unsaved service edits and keys', async ({page}) => {
  await settingsFixture(page, false, 'Custom service', true);
  await page.getByRole('button',{name:'Services',exact:true}).click();
  await page.getByLabel('Service name',{exact:true}).fill('Unsaved synthetic name');
  await page.getByLabel('API Key',{exact:true}).fill('synthetic-only-new-key');
  await page.evaluate(()=>{const w=window as any;w.pendingRoute={section:'translation',profile_id:null};w.emit('settings-navigate',null);});
  await expect(page.getByRole('heading',{name:'Translation',exact:true})).toBeVisible();
  await page.evaluate(()=>{const w=window as any;w.pendingRoute={section:'services',profile_id:'default'};w.emit('settings-navigate',null);});
  await expect(page.getByLabel('Service name',{exact:true})).toHaveValue('Unsaved synthetic name');
  await expect(page.getByLabel('API Key',{exact:true})).toHaveValue('synthetic-only-new-key');
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='save_configuration'))).toEqual([]);
});

test('first-use dictionary choice is an explicit draft until saved', async ({page}) => {
  await settingsFixture(page);
  await page.evaluate(()=>{const w=window as any;w.pendingRoute={section:'services',dictionary:true};w.emit('settings-navigate',null);});
  await expect(page.getByRole('button',{name:'Test dictionary',exact:true})).toBeVisible();
  await expect(page.getByRole('button',{name:'Save changes',exact:true})).toBeEnabled();
  expect(await page.evaluate(()=>(window as any).settings.profiles.find((p:any)=>p.id===(window as any).settings.active_profile).provider)).toBe('custom');
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).settings.profiles.find((p:any)=>p.id===(window as any).settings.active_profile).provider)).toBe('system_dictionary');
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>['test_connection','translate_text','retry_task'].includes(c.cmd)))).toEqual([]);
});

test('a settings deep link received before bootstrap opens the requested pane',async({page})=>{
  await settingsFixture(page,true);
  await page.evaluate(()=>{const w=window as any;w.pendingRoute={section:'services',profile_id:'default'};w.emit('settings-navigate',null);});
  await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='take_settings_route').length)).toBeGreaterThan(1);
  await page.evaluate(()=>(window as any).releaseBootstrap());
  await expect(page.getByLabel('Service name',{exact:true})).toBeVisible();
});


test('translation setup from a dictionary choice opens the model form without saving',async({page})=>{
  await settingsFixture(page);
  await page.evaluate(()=>{const w=window as any;w.pendingRoute={section:'services',dictionary:true};w.emit('settings-navigate',null);});
  await expect(page.getByRole('button',{name:'Test dictionary',exact:true})).toBeVisible();
  await page.evaluate(()=>{const w=window as any;w.pendingRoute={section:'services',translation:true};w.emit('settings-navigate',null);});
  await expect(page.getByLabel('API Key',{exact:true})).toBeVisible();
  await expect(page.getByRole('combobox',{name:'Model',exact:true})).toBeVisible();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='save_configuration'))).toEqual([]);
});


test('Zhipu onboarding only requires a key and saves the free model explicitly',async({page})=>{
 await settingsFixture(page);
 await page.evaluate(()=>{const w=window as any;w.pendingRoute={section:'services',zhipu:true};w.emit('settings-navigate',null);});
 const card=page.getByRole('region',{name:'Zhipu quick setup'});
 await expect(card).toBeVisible();
 await expect(card.locator('input')).toHaveCount(1);
 await expect(card.getByLabel('API Key',{exact:true})).toHaveAttribute('type','password');
 await expect(card.getByRole('button',{name:'Save and use'})).toBeDisabled();
 await card.getByRole('button',{name:'2. Get an API key'}).click();
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='open_translation_link').at(-1).args.url)).toBe('https://bigmodel.cn/usercenter/proj-mgmt/apikeys');
 await card.getByLabel('API Key',{exact:true}).fill('synthetic-zhipu-key');
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>['save_configuration','test_connection'].includes(c.cmd)))).toEqual([]);
 await card.getByRole('button',{name:'Save and use'}).click();
 const saved=await page.evaluate(()=>{const w=window as any;return w.settings.profiles.find((p:any)=>p.id===w.settings.active_profile)});
 expect(saved.provider).toBe('zhipu');expect(saved.model).toBe('glm-4.7-flash');
 expect(JSON.stringify(saved)).not.toContain('synthetic-zhipu-key');
 await expect(card.getByLabel('API Key',{exact:true})).toHaveValue('');
});

test('Zhipu quick setup preserves an existing nondefault model',async({page})=>{
 await settingsFixture(page);
 await page.getByRole('button',{name:'Services',exact:true}).click();
 await page.locator('.service-list').getByRole('button',{name:/Zhipu/}).click();
 await page.getByLabel('Model',{exact:true}).fill('existing-synthetic-model');
 await page.evaluate(()=>{const w=window as any;w.pendingRoute={section:'services',zhipu:true};w.emit('settings-navigate',null);});
 await expect(page.getByLabel('Model',{exact:true})).toHaveValue('existing-synthetic-model');
 await expect(page.getByRole('region',{name:'Zhipu quick setup'})).toHaveCount(0);
});


test('connection feedback is visible beside its button and distinguishes saved versus unsaved configuration',async({page})=>{
 await settingsFixture(page);await page.getByRole('button',{name:'Services',exact:true}).click();
 await page.evaluate(()=>{(window as any).deferTest=true;});
 await page.getByRole('button',{name:'Test connection',exact:true}).click();
 const feedback=page.locator('.service-test-feedback');
 await expect(feedback).toContainText('Testing connection');
 await expect(feedback).toBeInViewport();
 await expect(page.getByRole('button',{name:'Testing…',exact:true})).toBeDisabled();
 await page.evaluate(()=>{const w=window as any;w.deferTest=false;w.finishTest();});
 await expect(feedback).toContainText('Your saved configuration passed');
 await expect(feedback).toBeInViewport();
 await page.screenshot({path:'docs/images/first-use/connection-success.png',fullPage:true});
 await page.getByLabel('API Key',{exact:true}).fill('synthetic-new-key');
 await expect(feedback).toHaveCount(0);
 await page.getByRole('button',{name:'Test connection',exact:true}).click();
 await expect(feedback).toContainText('Save changes to apply');
 await page.evaluate(()=>{(window as any).testError='Synthetic HTTP 401';});
 await page.getByRole('button',{name:'Test connection',exact:true}).click();
 await expect(feedback).toContainText('Connection failed');
 await expect(feedback).toContainText('Synthetic HTTP 401');
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='save_configuration'))).toEqual([]);
});



test('append recorders capture key combinations, cancel safely and save cleared bindings explicitly',async({page})=>{
 await settingsFixture(page);
 await page.getByText('Optional append shortcuts',{exact:true}).click();
 const selection=page.getByRole('button',{name:'Record append selection shortcut',exact:true});
 const screenshot=page.getByRole('button',{name:'Record append screenshot shortcut',exact:true});
 await expect(page.locator('.optional-shortcuts input,.optional-shortcuts small,.append-shortcuts-note,.append-shortcuts-format')).toHaveCount(0);
 await selection.click();await expect(selection).toHaveClass(/recording/);
 await expect(page.getByRole('button',{name:'Save changes',exact:true})).toBeDisabled();
 await page.keyboard.down('Alt');await page.keyboard.down('Shift');await page.keyboard.down('D');
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='end_shortcut_recording').length)).toBe(0);
 await page.keyboard.up('D');await page.keyboard.up('Shift');await page.keyboard.up('Alt');
 await expect(selection).not.toHaveClass(/recording/);await expect(selection.locator('kbd')).toHaveText(['⌥','⇧','D']);
 await recordAppendShortcut(page,'screenshot','S');
 await selection.click();await page.keyboard.press('Escape');await expect(selection).not.toHaveClass(/recording/);await expect(selection.locator('kbd')).toHaveText(['⌥','⇧','D']);
 await screenshot.click();await page.keyboard.press('Tab');await expect(screenshot).not.toHaveClass(/recording/);await expect(screenshot.locator('kbd')).toHaveText(['⌥','⇧','S']);
 expect(await page.evaluate(()=>(window as any).settings.append_screenshot_shortcut??null)).toBeNull();
 await page.getByRole('button',{name:'Save changes',exact:true}).click();
 expect(await page.evaluate(()=>(window as any).settings.append_selection_shortcut)).toBe('Alt+Shift+D');
 expect(await page.evaluate(()=>(window as any).settings.append_screenshot_shortcut)).toBe('Alt+Shift+S');
 await expect(page.getByRole('button',{name:'Disable append selection shortcut',exact:true})).toHaveCount(0);
 await selection.click();
 await page.getByRole('button',{name:'Disable append selection shortcut',exact:true}).click();
 await expect(selection.locator('kbd')).toHaveCount(0);
 expect(await page.evaluate(()=>(window as any).settings.append_selection_shortcut)).toBe('Alt+Shift+D');
 await page.getByRole('button',{name:'Save changes',exact:true}).click();
 expect(await page.evaluate(()=>(window as any).settings.append_selection_shortcut)).toBeNull();
 expect(await page.evaluate(()=>(window as any).settings.append_screenshot_shortcut)).toBe('Alt+Shift+S');
 await screenshot.click();await expect(screenshot).toContainText('Press keys…');await page.keyboard.press('Delete');
 await expect(screenshot.locator('kbd')).toHaveCount(0);
 expect(await page.evaluate(()=>(window as any).settings.append_screenshot_shortcut)).toBe('Alt+Shift+S');
 await page.getByRole('button',{name:'Revert',exact:true}).click();
 for(const width of [1100,850,620]) {
  await page.setViewportSize({width,height:820});
  const a=await selection.boundingBox(),b=await screenshot.boundingBox();expect(b!.y-a!.y-a!.height).toBeGreaterThanOrEqual(14);
  for(const recorder of [selection,screenshot]){const outer=await recorder.locator('..').boundingBox(),inner=await recorder.boundingBox();expect(outer!.width-inner!.width).toBeLessThanOrEqual(6);}
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 }
 await page.setViewportSize({width:1100,height:820});
 await page.getByRole('button',{name:'Appearance',exact:true}).click();
 await page.getByRole('combobox',{name:'Display language',exact:true}).selectOption('zh');
 await page.getByRole('combobox',{name:'外观',exact:true}).selectOption('dark');
 await page.getByRole('button',{name:'翻译与快捷键',exact:true}).click();
 await page.locator('.optional-shortcuts summary').click();
 await page.locator('.optional-shortcuts').screenshot({path:'.ai-notes/append-recorders-dark.png',animations:'disabled'});
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>['translate_text','start_capture','retry_task'].includes(c.cmd)))).toEqual([]);
});


test('revert during delayed recorder cleanup preserves restored shortcuts and focus',async({page})=>{
 await settingsFixture(page);await page.getByText('Optional append shortcuts',{exact:true}).click();
 await recordAppendShortcut(page,'selection','D');await page.getByRole('button',{name:'Save changes',exact:true}).click();
 await page.getByLabel('Default translation language',{exact:true}).selectOption('fr');
 const selection=page.getByRole('button',{name:'Record append selection shortcut',exact:true});
 await selection.click();await expect(selection).toContainText('Press keys…');
 await page.evaluate(()=>{(window as any).deferEndRecording=true;});
 await page.getByRole('button',{name:'Disable append selection shortcut',exact:true}).click();
 await page.getByRole('button',{name:'Revert',exact:true}).click();
 const language=page.getByLabel('Default translation language',{exact:true});await language.focus();
 await page.evaluate(()=>(window as any).releaseRecording());
 await expect(selection).not.toHaveClass(/recording/);await expect(selection.locator('kbd')).toHaveText(['⌥','⇧','D']);
 await expect(language).toBeFocused();await expect(page.getByRole('button',{name:'Save changes',exact:true})).toBeDisabled();
});


test('notebook prioritizes saved target-language meanings and folds the original dictionary entry',async({page},testInfo)=>{
 await settingsFixture(page);
 await page.evaluate(()=>{const w=window as any;w.vocabulary[0].contexts[0].meanings={
  'zh-Hans':{general:'有韧性的',contextual:'能适应变化并恢复的',sentence_translation:'坚韧的团队能够适应变化。'},
  'fr':{general:'résilient',contextual:'capable de se rétablir',sentence_translation:'Une équipe résiliente.'}
 };});
 await page.getByRole('button',{name:'Vocabulary',exact:true}).click();
 await expect(page.locator('.word-list-copy')).toContainText('能适应变化并恢复的');
 await page.locator('.vocabulary-item').click();
 await expect(page.locator('.word-meanings')).toContainText('有韧性的');
 await expect(page.locator('.word-meanings')).not.toContainText('capable de se rétablir');
 const original=page.locator('.word-meanings details');
 await expect(original).not.toHaveAttribute('open','');
 await expect(original.locator('p')).not.toBeVisible();
 await original.locator('summary').click();await expect(original.locator('p')).toHaveText('able to recover quickly');
 await page.evaluate(()=>{const w=window as any;const second=structuredClone(w.vocabulary[0]);second.id='word-2';second.word='credential';second.dict_meaning='An identity document';second.contexts[0].surface='credential';second.contexts[0].context_sentence='Use a credential.';second.contexts[0].meanings={'zh-Hans':{general:'凭据',contextual:'认证凭据',sentence_translation:'使用凭据。'}};w.vocabulary.push(second);w.emit('vocabulary-changed',null);});
 await page.locator('.vocabulary-item').filter({hasText:'credential'}).click();
 await expect(page.locator('.word-meanings details')).not.toHaveAttribute('open','');
 await page.locator('.vocabulary-item').filter({hasText:'resilient'}).click();
 await expect(page.locator('.word-meanings details')).not.toHaveAttribute('open','');

 await page.locator('.vocabulary-workspace').screenshot({path:testInfo.outputPath('target-language-notebook.png')});
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='vocabulary_meaning'))).toEqual([]);
 await page.getByRole('searchbox',{name:'Search vocabulary'}).fill('有韧性的');
 await expect(page.locator('.vocabulary-item')).toHaveCount(1);
});
