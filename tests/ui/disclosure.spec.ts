import { test, expect, type Page } from '@playwright/test';

async function editableSource(page: Page) {
  const edit = page.getByRole('button', {name:'Edit source',exact:true});
  if (await edit.count() && await edit.isEnabled()) await edit.click();
  return page.getByRole('textbox', {name:'Source text',exact:true});
}

async function fixture(page: Page, initiallySelected=true, initialSourceLanguage='') {
  await page.addInitScript(({initialSelection, initialSourceLanguage}) => {
    const w = window as any;
    w.isTauri = true;
    w.calls = [];w.words=[];
    w.keychainRequired = false;
    w.keychainError = '';
    w.deferKeychain = false;
    w.speechVoices = [{locale:'en-GB',available:true},{locale:'en-US',available:true}];
    w.previewResults = {};w.deferredPreviewWords=[];w.previewResolvers=[];
    w.lookupFailure='';w.deferLookup=false;w.lookupResolvers=[];
    w.guidanceResponses={};w.deferGuidance=false;w.guidanceResolvers=[];
    w.detectedLanguage='en';
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    const callbacks: Record<number, Function> = {}, events: Record<string, number> = {};
    let callback = 0;
    w.settings = {
      profiles: [{ id: 'default', provider: 'custom', name: 'Custom service', model: 'gpt-6-luna', revision: 1 }],
      active_profile: 'default', target_language: 'zh-Hans', ui_language: 'en',
      source_language: initialSourceLanguage,
      theme: 'light', ui_font_size: 14, font_size: 16, auto_hide: false,
    };
    w.task = {
      id: 'fixture', revision: 1, attempt: 1, source: 'selection', status: 'ready',
      original_text: 'Selected model is at capacity. Please try a different model.',
      edited_text: null, translation: '', profile: w.settings.profiles[0],
      source_language: null, detected_language: 'en', target_language: 'zh-Hans',
      error: null, has_image: false, timings: {}, past_attempts: [],
    };
    if(!initialSelection)Object.assign(w.task,{status:'completed',translation:'Saved history text'});
    const snapshot = () => ({ type: 'snapshot', snapshot: { revision: w.task.revision, tasks: [structuredClone(w.task)] }, selected: initialSelection?w.task.id:null, notice: null });
    w.complete = () => {
      Object.assign(w.task, { status: 'completed', translation: '模型当前已满，请尝试其他模型。', revision: w.task.revision + 1 });
      w.wire(snapshot());
    };
    w.emit = (event: string, payload: unknown) => callbacks[events[event]]?.({ event, payload });
    w.__TAURI_INTERNALS__ = {
      transformCallback: (fn: Function) => { callbacks[++callback] = fn; return callback; },
      unregisterCallback: (id: number) => { delete callbacks[id]; },
      invoke: async (cmd: string, args: any) => {
        w.calls.push({ cmd, args });
        if (cmd === 'plugin:event|listen') { events[args.event] = args.handler; return callback; }
        if (cmd === 'bootstrap') return { ...snapshot(), configuration: { settings: w.settings }, permissions: { accessibility: true, screen: true }, pinned: false, expanded: false };
        if (cmd === 'configuration_settings') return w.settings;
        if (cmd === 'permissions') return { accessibility: true, screen: true };
        if (cmd === 'credential_authorization_required') return w.keychainRequired;
        if (cmd === 'authorize_saved_credential') {
          if (w.keychainError) throw w.keychainError;
          if (w.deferKeychain) return new Promise(resolve => { w.finishKeychain = () => { w.keychainRequired = false; resolve(null); }; });
          w.keychainRequired = false;
          return null;
        }
        if (cmd === 'dictionary_guidance') {
          if(w.deferGuidance)return new Promise(resolve=>w.guidanceResolvers.push({text:args.text,resolve}));
          return structuredClone(w.guidanceResponses[args.text] ?? {issue:null,profile_ids:[]});
        }
        if(cmd==='source_update'){Object.assign(w.task,{edited_text:args.request.text,source_language:args.request.source_language,target_language:args.request.target_language,source_revision:(w.task.source_revision??0)+1,revision:w.task.revision+1});w.wire(snapshot());return structuredClone(w.task);}
        if(cmd==='vocabulary_list')return structuredClone(w.words);
        if(cmd==='vocabulary_lookup'){
          if(w.deferLookup)return new Promise((resolve,reject)=>w.lookupResolvers.push({words:args.words,resolve,reject}));
          if(w.lookupFailure)throw w.lookupFailure;
          const keys=new Set(args.words.map((word:string)=>word.trim().toLowerCase()));
          return structuredClone(w.words.filter((item:any)=>keys.has(item.word.toLowerCase())));
        }
        if(cmd==='speech_voices')return structuredClone(w.speechVoices);
        if(cmd==='vocabulary_preview'){
          const word=args.sourceText.slice(args.selectionStart,args.selectionEnd).trim();
          const result=w.previewResults[word] ?? {candidate:{word:word.toLowerCase(),surface:word,phonetic:'',pos:'n.',dict_meaning:'Synthetic local definition',context_meaning:'',context_sentence:args.sourceText,sentence_trans:'',difficulty:''},translation_highlights:[]};
          if(w.deferredPreviewWords.includes(word))return new Promise(resolve=>w.previewResolvers.push({word,resolve}));
          return structuredClone(result);
        }
        if(cmd==='vocabulary_collect'){
          const c=args.candidate;let item=w.words.find((i:any)=>i.word===c.word);const norm=(s:string)=>s.trim().replace(/\s+/g,' ');
          if(!item){item={...c,id:'word-'+w.words.length,review_count:0,status:'learning',contexts:[]};w.words.push(item)}
          if(!item.contexts.some((v:any)=>norm(v.context_sentence)===norm(c.context_sentence))){item.review_count++;item.contexts.push({...c,id:'context-'+item.review_count});}
          item.status='learning';return structuredClone(item);
        }
        if (cmd === 'detect_text_language') return w.detectedLanguage;
        if (cmd === 'subscribe') { w.wire = (m: unknown) => args.onEvent.onmessage(m); return 'fixture'; }
        if (cmd === 'translate_text') {
          const profile=w.settings.profiles.find((p:any)=>p.id===(args.profileId??w.settings.active_profile));
          Object.assign(w.task,{id:'manual-fixture',source:'manual',profile:structuredClone(profile),status:'queued',original_text:args.text,edited_text:null,error:null,source_language:args.sourceLanguage,target_language:args.targetLanguage,revision:w.task.revision+1});
          w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id});
          return w.task.id;
        }
        if (cmd === 'retry_task') {
          if(args.profileId)w.task.profile=structuredClone(w.settings.profiles.find((p:any)=>p.id===args.profileId));
          if (w.task.status !== 'ready') w.task.attempt++;
          Object.assign(w.task, { status: 'queued', edited_text: args.editedText, source_language: args.sourceLanguage, target_language: args.targetLanguage, translation: '', revision: w.task.revision + 1 });
          w.wire(snapshot());
        }
        if (cmd === 'stop_task') {
          Object.assign(w.task, { status: w.task.vocabulary_state==='extracting'?'completed':'stopped', vocabulary_state:'cancelled', revision: w.task.revision + 1 });
          w.wire(snapshot());
        }
        return null;
      },
    };
  },{initialSelection:initiallySelected,initialSourceLanguage});
  await page.goto('/');
  if(initiallySelected)await expect(page.locator('.disclosure-control')).toBeEnabled();else await expect(page.locator('.disclosure-control')).toBeDisabled();
}

const requests = (page: Page) => page.evaluate(() => (window as any).calls.filter((c: any) => ['retry_task','translate_text'].includes(c.cmd)).length);
const complete = (page: Page) => page.evaluate(() => (window as any).complete());

for (const source of ['selection', 'screenshot']) {
  for (const key of ['d', 's']) {
    test(`${key.toUpperCase()} waits for ${source} source and a fresh keypress`, async ({ page }) => {
      await fixture(page);
      await page.evaluate(source => {
        const w = window as any;
        Object.assign(w.task, {source, status:source === 'screenshot' ? 'recognizing' : 'capturing', original_text:'Partial synthetic source', revision:2});
        w.wire({type:'snapshot',snapshot:{revision:2,tasks:[structuredClone(w.task)]},selected:w.task.id});
      }, source);
      const toggle = page.locator('.disclosure-control');
      await expect(toggle).toBeDisabled();
      await page.keyboard.down(key);
      expect(await requests(page)).toBe(0);
      await page.evaluate(() => {
        const w = window as any;
        Object.assign(w.task, {status:'ready',original_text:'Complete synthetic source',revision:3});
        w.wire({type:'snapshot',snapshot:{revision:3,tasks:[structuredClone(w.task)]},selected:w.task.id});
      });
      await expect(toggle).toBeEnabled();
      await page.keyboard.down(key); // browser marks a held key as repeat
      expect(await requests(page)).toBe(0);
      await page.keyboard.up(key);
      await page.keyboard.press('x');
      expect(await requests(page)).toBe(0);
      await page.keyboard.press(key);
      await expect.poll(() => requests(page)).toBe(1);
      expect(await page.evaluate(() => (window as any).calls.find((c:any)=>c.cmd==='retry_task').args.editedText)).toBe('Complete synthetic source');
    });
  }
}

test('saved key authorization explains Keychain and resumes only after confirmation', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { (window as any).keychainRequired = true; });
  await page.locator('.disclosure-control').click();
  const dialog = page.getByRole('dialog', {name:'Authorize saved API key'});
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText('macOS Keychain');
  await expect(dialog.locator('input')).toHaveCount(0);
  expect(await requests(page)).toBe(0);
  await dialog.getByRole('button', {name:'Cancel',exact:true}).click();
  expect(await page.evaluate(() => (window as any).calls.filter((c:any)=>c.cmd==='authorize_saved_credential').length)).toBe(0);
  await page.locator('.disclosure-control').click();
  await dialog.getByRole('button', {name:'Authorize and continue',exact:true}).click();
  await expect(dialog).not.toBeVisible();
  await expect.poll(() => requests(page)).toBe(1);
  expect(await page.evaluate(() => (window as any).calls.filter((c:any)=>c.cmd==='authorize_saved_credential').map((c:any)=>c.args))).toEqual([{profileId:'default'}]);
});

test('denied system authorization does not submit or repeatedly request the key', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { const w=window as any; w.keychainRequired=true; w.keychainError='Synthetic cancelled authorization'; });
  await page.keyboard.press('s');
  const dialog=page.getByRole('dialog',{name:'Authorize saved API key'});
  await dialog.getByRole('button',{name:'Authorize and continue',exact:true}).click();
  await expect(dialog.getByRole('alert')).toContainText('Synthetic cancelled authorization');
  await page.keyboard.press('d');
  await page.keyboard.press('s');
  expect(await requests(page)).toBe(0);
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='authorize_saved_credential').length)).toBe(1);
  await page.keyboard.press('Escape');
  await expect(dialog).not.toBeVisible();
  expect(await requests(page)).toBe(0);
});

test('a new source during system authorization cancels automatic translation of the old source', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { const w=window as any; w.keychainRequired=true; w.deferKeychain=true; });
  await page.keyboard.press('d');
  const dialog=page.getByRole('dialog',{name:'Authorize saved API key'});
  await dialog.getByRole('button',{name:'Authorize and continue',exact:true}).click();
  await expect(dialog.getByRole('button',{name:'Waiting for authorization…'})).toBeDisabled();
  await page.evaluate(() => {
    const w=window as any;
    Object.assign(w.task,{id:'new-source',original_text:'A different synthetic source.',source_revision:2,revision:2});
    w.wire({type:'snapshot',snapshot:{revision:2,tasks:[structuredClone(w.task)]},selected:w.task.id});
  });
  await expect(dialog).not.toBeVisible();
  await page.evaluate(() => (window as any).finishKeychain());
  await expect(page.locator('.disclosure-control')).toBeEnabled();
  expect(await requests(page)).toBe(0);
  await page.keyboard.press('s');
  await expect.poll(()=>requests(page)).toBe(1);
  expect(await page.evaluate(()=>(window as any).calls.find((c:any)=>c.cmd==='retry_task').args.id)).toBe('new-source');
});

test('D and S do not translate while editing, composing or using modifiers', async ({ page }) => {
  await fixture(page);
  const editor=await editableSource(page);
  await editor.fill('Synthetic editable source.');
  await editor.press('End');
  await page.keyboard.type('ds');
  await expect(editor).toHaveValue('Synthetic editable source.ds');
  await page.getByRole('button',{name:'Finish editing',exact:true}).click();
  for(const key of ['d','s']) {
    await page.keyboard.press(`Alt+${key}`);
    await page.keyboard.press(`Control+${key}`);
    await page.keyboard.press(`Meta+${key}`);
    await page.evaluate(key=>window.dispatchEvent(new KeyboardEvent('keydown',{key,isComposing:true,bubbles:true})),key);
  }
  expect(await requests(page)).toBe(0);
  await page.keyboard.press('d');
  await expect.poll(()=>requests(page)).toBe(1);
});

test('Keychain explanation is localized and fits a narrow dark window', async ({ page }) => {
  await fixture(page);
  await page.setViewportSize({width:380,height:720});
  await page.evaluate(()=>{const w=window as any; w.keychainRequired=true;w.settings.ui_language='zh';w.settings.theme='dark';w.emit('configuration-changed',w.settings);});
  await page.locator('.disclosure-control').click();
  const dialog=page.getByRole('dialog',{name:'授权读取已保存的 API Key'});
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText('已加密保存在 macOS 系统钥匙串');
  const bounds=await dialog.boundingBox();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.x+bounds!.width).toBeLessThanOrEqual(380);
  expect(bounds!.y+bounds!.height).toBeLessThanOrEqual(720);
  await expect(dialog.getByRole('button',{name:'授权并继续'})).toBeInViewport();
  await page.screenshot({path:'artifacts/keychain-authorization-dark.png'});
});

test('arrow reflects state; D/S and button toggle, retain result and detect edits', async ({ page }) => {
  await fixture(page);
  const toggle = page.locator('.disclosure-control');
  await expect(toggle).toHaveAttribute('aria-keyshortcuts', 'D S');
  await expect(page.locator('.capture-hint kbd')).toHaveText('D / S');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  // The current state uses a left chevron when collapsed and down when open.
  await expect(toggle.locator('svg')).toHaveCSS('transform', 'matrix(-1, 0, 0, -1, 0, 0)');
  await toggle.click();
  await expect.poll(() => requests(page)).toBe(1);
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await expect(toggle.locator('svg')).toHaveCSS('transform', 'matrix(0, 1, -1, 0, 0, 0)');
  await page.keyboard.press('d');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await complete(page);
  await expect(page.locator('#translation-body')).toBeHidden();
  await page.keyboard.press('d');
  await expect(page.locator('.translation-prose')).toHaveText('模型当前已满，请尝试其他模型。');
  await toggle.click();
  await toggle.click();
  expect(await requests(page)).toBe(1);
  const editor = (await editableSource(page));
  await editor.click();
  await page.keyboard.press('End');
  await page.keyboard.press('d');
  expect(await editor.inputValue()).toMatch(/d$/);
  expect(await requests(page)).toBe(1);
  await toggle.click();
  await expect.poll(() => requests(page)).toBe(2);
  await complete(page);
  // Restoring the original after translating an edit must send the actual original.
  await editor.fill('Selected model is at capacity. Please try a different model.');
  await toggle.click();
  await expect.poll(() => requests(page)).toBe(3);
  expect(await page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'retry_task').at(-1).args.editedText)).toBe('Selected model is at capacity. Please try a different model.');
});

test('loading control remains visible when collapsed, explains cancellation and stops the task', async ({ page }) => {
  await fixture(page);
  const toggle = page.locator('.disclosure-control');
  await toggle.click();
  const stop = page.getByRole('button', { name: 'Stop translation', exact: true });
  await expect(stop.locator('.translation-loader')).toBeVisible();
  await toggle.click();
  await expect(page.locator('#translation-body')).toBeHidden();
  await expect(toggle.locator('svg')).toHaveCSS('transform', 'matrix(-1, 0, 0, -1, 0, 0)');
  await stop.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Stop translation');
  await stop.click();
  await expect(stop).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'stop_task').length)).toBe(1);
  expect(await requests(page)).toBe(1);
});

test('service rail uses one interface font size, including model tag and language control', async ({ page }) => {
  await fixture(page);
  for (const size of [14, 18]) {
    await page.evaluate(ui_font_size => { const w = window as any; w.settings.ui_font_size = ui_font_size; w.emit('configuration-changed', w.settings); }, size);
    const fonts = await page.evaluate(() => ['.provider-name', '.model-name', '.target-language > span', '.target-language select'].map(selector => getComputedStyle(document.querySelector(selector)!).fontSize));
    expect(fonts).toEqual(Array(4).fill(`${size}px`));
  }
});

test('source editing retains focus across app reactivation; language controls do not translate', async ({ page }) => {
  await fixture(page);
  await page.getByLabel('Translation language', { exact: true }).selectOption('fr');
  await page.getByLabel('Translation language', { exact: true }).focus();
  await page.keyboard.press('d');
  expect(await requests(page)).toBe(0);
  const editor = (await editableSource(page));
  await editor.click();
  await page.evaluate(() => { window.dispatchEvent(new Event('blur')); window.dispatchEvent(new Event('focus')); });
  await page.keyboard.press('d');
  expect(await editor.inputValue()).toMatch(/d$/);
  expect(await requests(page)).toBe(0);
  await page.locator('.window-name').click();
  await page.keyboard.press('d');
  expect(await requests(page)).toBe(1);
});

for (const source of ['selection', 'screenshot']) {
  test(`Escape hides a ${source} result while preserving the draft and active translation`, async ({ page }) => {
    await fixture(page);
    const editor = (await editableSource(page));
    await editor.fill('Synthetic edited source');
    await page.evaluate(source => { (window as any).task.source = source; }, source);
    await page.locator('.disclosure-control').click();
    await page.evaluate(() => {
      const w = window as any;
      Object.assign(w.task, { status: 'translating', translation: 'Partial translation', revision: w.task.revision + 1 });
      w.wire({ type: 'events', events: [{ type: 'changed', task: structuredClone(w.task) }] });
    });
    await page.getByRole('button', { name: 'Pin', exact: true }).click();
    await editor.focus();
    await page.keyboard.press('Escape');
    await expect.poll(() => page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'hide_window').length)).toBe(1);
    await expect(editor).toHaveValue('Synthetic edited source');
    await expect(page.getByRole('button', { name: 'Unpin', exact: true })).toBeVisible();
    expect(await page.evaluate(() => ({ status: (window as any).task.status, translation: (window as any).task.translation }))).toEqual({ status: 'translating', translation: 'Partial translation' });
    expect(await page.evaluate(() => (window as any).calls.filter((c: any) => ['stop_task', 'remove_task'].includes(c.cmd)))).toEqual([]);
    expect(await requests(page)).toBe(1);
    await complete(page);
    expect(await page.evaluate(() => (window as any).task.status)).toBe('completed');
    await expect(editor).toHaveValue('Synthetic edited source');
  });
}

test('Escape hides a ready result without translating and advertises its shortcut', async ({ page }) => {
  await fixture(page);
  const hide = page.getByRole('button', { name: 'Hide', exact: true });
  await expect(hide).toHaveAttribute('aria-keyshortcuts', 'Escape');
  await expect(hide).toHaveAttribute('title', 'Hide (Esc)');
  const editor = (await editableSource(page));
  await editor.fill('Unsubmitted synthetic draft');
  await page.keyboard.press('Escape');
  await expect.poll(() => page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'hide_window').length)).toBe(1);
  expect(await requests(page)).toBe(0);
  await expect(editor).toHaveValue('Unsubmitted synthetic draft');
  expect(await page.evaluate(() => (window as any).task.status)).toBe('ready');
});

test('Escape respects modifiers, composition, repeats, dialogs and native select menus', async ({ page }) => {
  await fixture(page);
  for (const key of ['Shift+Escape', 'Control+Escape', 'Alt+Escape', 'Meta+Escape']) await page.keyboard.press(key);
  await page.evaluate(() => {
    for (const options of [{ repeat: true }, { isComposing: true }]) {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true, ...options }));
    }
    const consumed = new KeyboardEvent('keydown', { key: 'Escape', cancelable: true });
    consumed.preventDefault();
    window.dispatchEvent(consumed);
    const dialog = document.createElement('dialog');
    dialog.textContent = 'Synthetic dialog';
    document.body.append(dialog);
    dialog.showModal();
  });
  await page.keyboard.press('Escape');
  await page.evaluate(() => document.querySelector('dialog')?.remove());
  await page.getByLabel('Translation language', { exact: true }).focus();
  await page.keyboard.press('Escape');
  expect(await page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'hide_window'))).toEqual([]);
  await page.locator('.disclosure-control').focus();
  await page.keyboard.press('Escape');
  await expect.poll(() => page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'hide_window').length)).toBe(1);
});

test('target label, select and disclosure never overlap at supported sizes', async ({ page }) => {
  await fixture(page);
  for (const width of [360, 460, 680]) {
    await page.setViewportSize({ width, height: 650 });
    await page.getByLabel('Translation language', { exact: true }).focus();
    const label = await page.locator('.target-language > span').boundingBox();
    const select = await page.locator('.target-language select').boundingBox();
    const button = await page.locator('.disclosure-control').boundingBox();
    expect(label!.x + label!.width + 5).toBeLessThanOrEqual(select!.x);
    expect(select!.x + select!.width + 5).toBeLessThanOrEqual(button!.x);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  await page.setViewportSize({ width: 460, height: 650 });
  await expect(page.getByLabel('Source language', { exact: true })).toContainText('Auto · English');
  await page.screenshot({ path: 'test-results/ready-light.png' });
  await page.locator('.disclosure-control').click();
  await complete(page);
  await page.screenshot({ path: 'test-results/translated-light.png' });
  await page.evaluate(() => { const w = window as any; w.settings.theme = 'dark'; w.emit('configuration-changed', w.settings); });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.screenshot({ path: 'test-results/translated-dark.png' });
});

test('language changes invalidate the result, errors reopen without automatic retries', async ({ page }) => {
  await fixture(page);
  const toggle = page.locator('.disclosure-control');
  await toggle.click();
  await complete(page);
  await page.getByLabel('Translation language', { exact: true }).selectOption('fr');
  expect(await requests(page)).toBe(1);
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await toggle.click();
  expect(await requests(page)).toBe(2);
  await complete(page);
  await page.getByLabel('Source language', { exact: true }).selectOption('fr');
  await toggle.click();
  expect(await requests(page)).toBe(3);
  await page.evaluate(() => {
    const w = window as any;
    Object.assign(w.task, { status: 'failed', error: 'Fixture service unavailable', revision: w.task.revision + 1 });
    w.wire({ type: 'snapshot', snapshot: { revision: w.task.revision, tasks: [w.task] }, selected: w.task.id, notice: null });
  });
  await expect(page.getByRole('alert')).toHaveText('Fixture service unavailable');
  await toggle.click();
  await page.keyboard.press('d');
  await expect(page.getByRole('alert')).toHaveText('Fixture service unavailable');
  expect(await requests(page)).toBe(3);
});

test('a failed second capture clears previous content; later captures select their own source', async ({ page }) => {
  await fixture(page);
  await page.locator('.disclosure-control').click();
  await complete(page);
  await page.evaluate(() => {
    const w = window as any;
    w.wire({ type: 'snapshot', snapshot: { revision: 4, tasks: [w.task] }, selected: null, notice: '请在其他应用中选中文字，再按划词快捷键。' });
  });
  await expect((await editableSource(page))).toHaveValue('');
  await expect(page.locator('.translation-prose')).toHaveCount(0);
  for (const source of ['The second selection.', 'The third selection.']) {
    await page.evaluate((text) => {
      const w = window as any;
      const task = { ...w.task, id: text, status: 'ready', original_text: text, edited_text: null, translation: '', revision: 5 };
      w.wire({ type: 'snapshot', snapshot: { revision: 5, tasks: [w.task, task] }, selected: task.id, notice: null });
    }, source);
    await expect((await editableSource(page))).toHaveValue(source);
    await expect(page.locator('#translation-body')).toHaveCount(0);
  }
});

test('screenshot generation announces progress until source text is ready without translating',async({page})=>{
  await fixture(page,false);
  await page.setViewportSize({width:680,height:700});
  await page.evaluate(()=>{
    const w=window as any;
    w.task={...w.task,id:'ocr-progress',source:'screenshot',status:'capturing',original_text:'',edited_text:null,translation:'',error:null,revision:2};
    w.wire({type:'snapshot',snapshot:{revision:2,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  const editor=(await editableSource(page));
  await expect(editor).toHaveAttribute('readonly','');
  if(await page.locator('.source-processing').count()===0)await page.locator('.source-panel').screenshot({path:'artifacts/previews/screenshot-progress-before.png'});
  await expect(page.getByRole('status')).toHaveText('Preparing screenshot…');
  await expect(editor).toHaveAttribute('aria-busy','true');
  await page.evaluate(()=>{
    const w=window as any;Object.assign(w.task,{status:'recognizing',revision:3});
    w.wire({type:'snapshot',snapshot:{revision:3,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  await expect(page.getByRole('status')).toHaveText('Recognizing screenshot text…');
  await expect(editor).toHaveValue('');
  await expect(editor).toHaveAttribute('placeholder','');
  await expect(page.locator('.source-image')).toHaveCount(0);
  await page.locator('.source-panel').screenshot({path:'artifacts/previews/screenshot-progress-after.png'});
  await page.keyboard.press('d');
  expect(await requests(page)).toBe(0);
  await page.evaluate(()=>{
    const w=window as any;Object.assign(w.task,{status:'ready',original_text:'Yurelay screenshot OCR test.',revision:4});
    w.wire({type:'snapshot',snapshot:{revision:4,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  await expect(page.getByRole('textbox',{name:'Source reading',exact:true})).toHaveText('Yurelay screenshot OCR test.');
  await editableSource(page);
  await expect(editor).toHaveValue('Yurelay screenshot OCR test.');
  await expect(editor).toHaveAttribute('aria-busy','false');
  await expect(editor).not.toHaveAttribute('readonly');
  await expect(page.locator('.source-processing')).toHaveCount(0);
  await expect(page.locator('#translation-body')).toHaveCount(0);
  expect(await requests(page)).toBe(0);
});

test('screenshot OCR failures remain visible while translation is collapsed without sending a request',async({page})=>{
  await fixture(page,false);
  await page.evaluate(()=>{
    const w=window as any;
    w.task={...w.task,id:'ocr-failure',source:'screenshot',status:'recognizing',original_text:'',edited_text:null,translation:'',error:null,revision:2};
    w.wire({type:'snapshot',snapshot:{revision:2,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  await expect(page.getByLabel('Source language',{exact:true})).toBeDisabled();
  await page.evaluate(()=>{
    const w=window as any;
    Object.assign(w.task,{status:'failed',error:'本地文字识别失败，请缩小选区后重试。',revision:3});
    w.wire({type:'snapshot',snapshot:{revision:3,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  const toggle=page.locator('.disclosure-control');
  await expect(toggle).toHaveAttribute('aria-expanded','false');
  await expect(page.getByRole('alert')).toHaveText('Local text recognition failed. Try a smaller selection.');
  await expect((await editableSource(page))).toHaveValue('');
  expect(await requests(page)).toBe(0);
  await page.locator('.window-name').click();
  await page.keyboard.press('d');
  await page.keyboard.press('s');
  await expect(toggle).toHaveAttribute('aria-expanded','false');
  await toggle.click();
  await expect(page.getByRole('alert')).toHaveCount(1);
  await expect(page.getByRole('alert')).toBeVisible();
  await toggle.click();
  await expect(page.getByRole('alert')).toBeVisible();
  expect(await requests(page)).toBe(0);
  await page.evaluate(()=>{
    const w=window as any;
    Object.assign(w.task,{id:'next-capture',status:'ready',original_text:'Yurelay screenshot OCR test.',error:null,revision:4});
    w.wire({type:'snapshot',snapshot:{revision:4,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect((await editableSource(page))).toHaveValue('Yurelay screenshot OCR test.');
});

test('OCR loading keeps the keyboard focus ring visibly painted',async({page})=>{
  await fixture(page,false);
  await page.setViewportSize({width:680,height:700});
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.evaluate(()=>{
    const w=window as any;
    w.task={...w.task,id:'ocr-focus',source:'screenshot',status:'recognizing',original_text:'',edited_text:null,translation:'',error:null,revision:2};
    w.wire({type:'snapshot',snapshot:{revision:2,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  await expect(page.locator('.source-processing')).toBeVisible();
  const frame=(await page.locator('.source-editor-frame').boundingBox())!;
  const clip={x:frame.x,y:frame.y+12,width:2,height:frame.height-24};
  const unfocused=await page.screenshot({clip});
  const editor=(await editableSource(page));
  for(let index=0;index<8 && !await editor.evaluate(el=>el===document.activeElement);index++)await page.keyboard.press('Tab');
  await expect(editor).toBeFocused();
  expect(await editor.evaluate(el=>el.matches(':focus-visible'))).toBe(true);
  const focused=await page.screenshot({clip});
  await page.locator('.source-panel').screenshot({path:'artifacts/previews/ocr-keyboard-focus.png'});
  expect(focused.equals(unfocused),'Tab focus must visibly change the source card border during OCR').toBe(false);
  await page.keyboard.press('Tab');
  expect((await page.screenshot({clip})).equals(unfocused)).toBe(true);
});

test('screenshot progress respects reduced motion, localization and narrow dark layouts',async({page})=>{
  await fixture(page,false);
  await page.emulateMedia({reducedMotion:'no-preference'});
  await page.evaluate(()=>{
    const w=window as any;
    w.settings.ui_language='zh';w.emit('configuration-changed',w.settings);
    w.task={...w.task,id:'ocr-layout',source:'screenshot',status:'recognizing',original_text:'',edited_text:null,translation:'',error:null,revision:2};
    w.wire({type:'snapshot',snapshot:{revision:2,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  await expect(page.getByRole('status')).toHaveText('正在识别截图文字…');
  const spinner=page.locator('.source-processing .translation-loader');
  await expect(spinner).toHaveCSS('animation-name','translation-spin');
  await page.emulateMedia({reducedMotion:'reduce'});
  await expect(spinner).toHaveCSS('animation-name','none');
  for(const theme of ['light','dark']){
    await page.evaluate(value=>{const w=window as any;w.settings.theme=value;w.emit('configuration-changed',w.settings);},theme);
    await expect(page.locator('html')).toHaveAttribute('data-theme',theme);
    for(const width of [580,680,980]){
      await page.setViewportSize({width,height:700});
      await expect(page.getByRole('status')).toBeVisible();
      const bounds=await page.evaluate(()=>{
        const frame=document.querySelector('.source-editor-frame')!.getBoundingClientRect();
        const progress=document.querySelector('.source-processing')!.getBoundingClientRect();
        return {frame:frame.toJSON(),progress:progress.toJSON(),overflow:document.documentElement.scrollWidth>innerWidth};
      });
      expect(bounds.overflow).toBe(false);
      expect(bounds.progress.left).toBeGreaterThanOrEqual(bounds.frame.left);
      expect(bounds.progress.right).toBeLessThanOrEqual(bounds.frame.right);
      expect(bounds.progress.top).toBeGreaterThanOrEqual(bounds.frame.top);
      expect(bounds.progress.bottom).toBeLessThanOrEqual(bounds.frame.bottom);
      await page.screenshot({path:`artifacts/previews/screenshot-progress-${theme}-${width}.png`});
    }
  }
  await page.evaluate(()=>{const w=window as any;w.wire({type:'snapshot',snapshot:{revision:3,tasks:[]},selected:null,notice:null});});
  await expect(page.locator('.source-processing')).toHaveCount(0);
  await expect(page.getByRole('textbox',{name:'原文内容'})).toHaveAttribute('aria-busy','false');
  expect(await requests(page)).toBe(0);
  await page.evaluate(()=>{
    const w=window as any;Object.assign(w.task,{sensitive:'API_SECRET',revision:4});
    w.wire({type:'snapshot',snapshot:{revision:4,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  await expect(page.locator('.sensitive-mask')).toBeVisible();
  await expect(page.locator('.source-processing')).toHaveCount(0);
  await expect(page.getByRole('textbox',{name:'原文内容'})).toHaveCount(0);
});


test('a snapshot without an explicit selection starts blank and history opens only on selection', async ({page}) => {
  await fixture(page);
  await page.evaluate(()=>{const w=window as any;w.complete();w.wire({type:'snapshot',snapshot:{revision:5,tasks:[w.task]},selected:null,notice:null});});
  await expect((await editableSource(page))).toHaveValue('');
  await expect(page.locator('.translation-prose')).toHaveCount(0);
  await expect(page.locator('.window-chrome img')).toHaveCount(0);
  await page.evaluate(()=>{const w=window as any;w.wire({type:'snapshot',snapshot:{revision:5,tasks:[w.task]},selected:w.task.id,notice:null});});
  await expect(page.locator('.translation-prose')).toHaveText('模型当前已满，请尝试其他模型。');
});


test('cold bootstrap keeps saved history out of the translation window', async ({page}) => {
  await fixture(page,false);
  await expect((await editableSource(page))).toHaveValue('');
  await expect(page.locator('.translation-prose')).toHaveCount(0);
  await expect(page.locator('.window-chrome img')).toHaveCount(0);
  expect(await requests(page)).toBe(0);
  await page.screenshot({path:'test-results/blank-startup.png'});
});

test('system dictionary uses definition controls and has no translation target', async ({page}) => {
  await fixture(page);
  await page.evaluate(()=>{const w=window as any;w.settings.profiles[0].provider='system_dictionary';w.settings.profiles[0].model='';w.emit('configuration-changed',w.settings);});
  await expect(page.getByLabel('Translation language',{exact:true})).toHaveCount(0);
  await page.locator('.disclosure-control').click();
  await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{status:'completed',translation:'a greeting',revision:w.task.revision+1});w.wire({type:'snapshot',snapshot:{revision:w.task.revision,tasks:[w.task]},selected:w.task.id,notice:null});});
  await expect(page.getByLabel('Definition',{exact:true})).toHaveText(/a greeting/);
  await expect(page.getByRole('button',{name:'Look up again',exact:true})).toBeVisible();
});

test('changing default service updates the current translation action', async ({ page }) => {
  await fixture(page);
  await complete(page);
  await page.evaluate(() => {
    const w = window as any;
    w.settings.profiles.push({id:'dictionary',provider:'system_dictionary',name:'System dictionary',model:'',revision:1});
    w.settings.active_profile='dictionary';
    w.emit('configuration-changed', structuredClone(w.settings));
  });
  await expect(page.locator('.provider-name')).toHaveText('System dictionary');
  await expect(page.locator('.disclosure-control')).toHaveAttribute('aria-label','Look up and expand (D / S)');
});

test('sensitive snapshot shows only the mask; D/S cannot bypass explicit consent',async({page})=>{
 await fixture(page);
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{sensitive:'MNEMONIC',original_text:'',translation:'',revision:2});w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null})});
 await expect((await editableSource(page))).toHaveCount(0);
 await expect(page.getByRole('alert')).toContainText('Possible sensitive information');await page.keyboard.press('d');expect(await requests(page)).toBe(0);
 await page.getByRole('button',{name:'Reveal source and translate'}).click();
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='resolve_sensitive').at(-1).args)).toEqual({id:'fixture',allow:true});
 await page.getByRole('button',{name:'Cancel and clear'}).click();
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='resolve_sensitive').at(-1).args.allow)).toBe(false);
 await page.screenshot({path:'artifacts/previews/2026-10-15-sensitive-mask.png'});
});


test('vocabulary cards collect once per context',async({page})=>{
 await fixture(page);await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{original_text:'A resilient team adapts to change.',status:'completed',translation:'坚韧的团队能够适应变化。',vocabulary:[{word:'resilient',surface:'resilient',phonetic:'/rɪˈzɪliənt/',pos:'adj.',dict_meaning:'坚韧',context_meaning:'坚韧的',context_sentence:'A resilient team adapts to change.',sentence_trans:'坚韧的团队能够适应变化。',difficulty:'B2'}],revision:2});w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null})});
 await page.locator('.disclosure-control').click();
 await page.setViewportSize({width:680,height:800});
 await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();
 await page.getByRole('button',{name:'Add',exact:true}).click();await expect(page.getByRole('button',{name:'Saved (×1)',exact:true})).toBeVisible();await expect(page.getByRole('button',{name:'Saved (×1)',exact:true})).toBeDisabled();
 await page.evaluate(()=>{const w=window as any;w.words[0].status='mastered';w.emit('vocabulary-changed',null)});await page.getByRole('button',{name:'Learn again',exact:true}).click();await expect(page.getByRole('button',{name:'Saved (×1)',exact:true})).toBeDisabled();
 await page.screenshot({path:'artifacts/previews/2026-10-15-vocabulary-cards.png'});
});

test('explicit sensitive consent expands the resulting translation without another request',async({page})=>{
 await fixture(page);await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{sensitive:'API_SECRET',original_text:'',revision:2});w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null})});
 await page.getByRole('button',{name:'Reveal source and translate'}).click();
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{sensitive:null,status:'completed',original_text:'synthetic fixture',translation:'Fixture result',revision:3});w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:3},selected:w.task.id,notice:null})});
 await expect(page.locator('.translation-prose')).toHaveText('Fixture result');expect(await requests(page)).toBe(0);
});


test('candidate arrival stays collapsed and explicit expansion preserves manual collapse and reading width',async({page})=>{
 await fixture(page);await page.setViewportSize({width:460,height:800});await page.locator('.disclosure-control').click();
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{status:'translating',extract_vocabulary:true,vocabulary_state:'none',translation:'译文',revision:2});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});
 await expect(page.getByRole('button',{name:'Copy translation',exact:true})).toBeDisabled();await expect(page.locator('.vocabulary-sidebar')).toHaveCount(0);
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{status:'completed',vocabulary_state:'extracting',revision:3});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});
 await expect(page.getByRole('button',{name:'Copy translation',exact:true})).toBeEnabled();await expect(page.locator('.translation-prose')).toHaveText('译文');const before=await page.locator('.translation-main').boundingBox();
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{vocabulary_state:'complete',vocabulary:Array.from({length:5},(_,i)=>({word:'word'+i,surface:'word'+i,phonetic:'',pos:'n.',context_meaning:'语境释义',context_sentence:'Sentence '+i,dict_meaning:'',sentence_trans:'',difficulty:'B2'})),revision:4});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});
 await expect(page.getByRole('button',{name:'Expand vocabulary',exact:true})).toBeVisible();await expect(page.locator('.vocabulary-side-card')).toHaveCount(0);
 await page.setViewportSize({width:680,height:800});await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();await expect(page.getByRole('button',{name:'Collapse vocabulary',exact:true})).toBeVisible();await expect(page.locator('.vocabulary-side-card')).toHaveCount(5);
 const after=await page.locator('.translation-main').boundingBox();expect(after!.width).toBe(before!.width);expect(after!.x).toBe(220);expect(Math.abs(after!.height-before!.height)).toBeLessThan(1);
 await page.getByRole('button',{name:'Collapse vocabulary',exact:true}).click();await expect(page.locator('.vocabulary-sidebar')).toHaveClass(/collapsed/);await expect(page.locator('.vocabulary-side-card')).toHaveCount(0);
 await page.evaluate(()=>{const w=window as any;w.task.revision++;w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});await expect(page.getByRole('button',{name:'Expand vocabulary',exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();await page.getByRole('button',{name:'View word0 meaning',exact:true}).click();await expect(page.locator('.vocabulary-row-detail')).toContainText('Sentence 0');
 await page.screenshot({path:'artifacts/previews/2026-10-15-vocabulary-left.png'});
 await page.getByRole('button',{name:'Collapse vocabulary',exact:true}).click();await page.evaluate(()=>{const w=window as any;w.task.attempt++;w.task.revision++;w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});await expect(page.getByRole('button',{name:'Expand vocabulary',exact:true})).toBeVisible();
 await page.evaluate(()=>{const w=window as any;w.task.vocabulary=[];w.task.revision++;w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});await expect(page.locator('.vocabulary-sidebar')).toHaveCount(0);expect(await requests(page)).toBe(1);
});

test('reading renders safe Markdown, preserves soft breaks, copies both formats and opens links through backend',async({page})=>{
 await fixture(page);
 const markdown='# 更新\n\n**功能**\n- 保留 `return_url`\n- [官方文档](https://example.test/docs)\n\n第一行\n第二行\n\n#16976\n\n```js\nconst x = 1;\n```\n\n<script>window.__attack=1</script>\n![tracking](https://example.test/tracking.png)\n[bad](javascript:alert(1))';
 await page.evaluate((translation)=>{const w=window as any;Object.assign(w.task,{status:'completed',translation,revision:2});w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null})},markdown);
 await page.locator('.disclosure-control').click();
 await expect(page.locator('.markdown-reading h1')).toHaveText('更新');await expect(page.locator('.markdown-reading li')).toHaveCount(2);await expect(page.locator('.markdown-reading pre code')).toContainText('const x = 1;');
 await expect(page.locator('.markdown-reading p').filter({hasText:'第一行'}).locator('br')).toHaveCount(1);await expect(page.locator('.markdown-reading')).toContainText('#16976');
 await expect(page.locator('.markdown-reading img,.markdown-reading script,.markdown-reading a[href^="javascript:"]')).toHaveCount(0);expect(await page.evaluate(()=>(window as any).__attack)).toBeUndefined();
 await page.getByRole('link',{name:'官方文档'}).click();expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='open_translation_link').at(-1).args.url)).toBe('https://example.test/docs');
 await page.getByRole('button',{name:'Copy translation',exact:true}).click();const plain=await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='copy_text').at(-1).args.text);expect(plain).toContain('• 保留 return_url');expect(plain).toContain('第一行\n第二行');expect(plain).not.toContain('```');
 await page.getByRole('button',{name:'Copy Markdown',exact:true}).click();expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='copy_text').at(-1).args.text)).toBe(markdown);
 await page.getByRole('button',{name:'Plain text',exact:true}).click();await expect(page.locator('.translation-prose')).toContainText('**功能**');await page.getByRole('button',{name:'Read',exact:true}).click();await expect(page.locator('.markdown-reading strong')).toHaveText('功能');expect(await requests(page)).toBe(0);
 await page.screenshot({path:'artifacts/previews/2026-10-15-markdown-reading.png'});
});

test('stream updates preserve stable nodes and defer replacing selected text until selection is released',async({page})=>{
 await fixture(page);await page.setViewportSize({width:460,height:800});await page.locator('.disclosure-control').click();
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{status:'translating',translation:'Stable first paragraph.\n\nStreaming **bo',revision:2});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});
 await expect(page.locator('.reading-block').first()).toHaveText('Stable first paragraph.');
 await page.locator('.reading-block').first().evaluate(el=>{(window as any).stableNode=el;const r=document.createRange();r.selectNodeContents(el);const s=getSelection()!;s.removeAllRanges();s.addRange(r);});
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{translation:'Stable first paragraph.\n\nStreaming **bold**\n\n```js\nconst x = 1;',revision:3});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});
 await page.waitForTimeout(100);expect(await page.evaluate(()=>getSelection()?.toString())).toBe('Stable first paragraph.');expect(await page.evaluate(()=>(window as any).stableNode===document.querySelector('.reading-block'))).toBe(true);
 await page.evaluate(()=>getSelection()?.removeAllRanges());await expect(page.locator('.markdown-reading strong')).toHaveText('bold');await expect(page.locator('.markdown-reading pre code')).toContainText('const x = 1;');
 expect(await page.evaluate(()=>(window as any).stableNode===document.querySelector('.reading-block'))).toBe(true);
});


test('left vocabulary rail supports dark mode, scrolling and manual collapse without overlays',async({page})=>{
 await fixture(page);await page.emulateMedia({reducedMotion:'reduce'});await page.setViewportSize({width:620,height:700});
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{status:'completed',translation:'Use OAuth2 for access.',vocabulary:[{word:'OAuth2',surface:'OAuth2',phonetic:'/invented/',pos:'term',context_meaning:'授权协议',dict_meaning:'通用授权框架',context_sentence:'Use OAuth2 for access.',sentence_trans:'使用 OAuth2 授权。',difficulty:'Term'}],revision:2});w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null});document.documentElement.setAttribute('data-theme','dark')});
 await page.locator('.disclosure-control').click();await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();await expect(page.getByRole('button',{name:'Collapse vocabulary',exact:true})).toBeVisible();await expect(page.locator('.vocabulary-ipa')).toHaveCount(0);
 await page.getByRole('button',{name:'View OAuth2 meaning',exact:true}).click();await expect(page.locator('.vocabulary-row-detail')).toContainText('通用授权框架');
 const layout=await page.evaluate(()=>{const panel=document.querySelector('.vocabulary-sidebar')!;return {overflow:panel.scrollWidth>panel.clientWidth,pageOverflow:document.documentElement.scrollWidth>innerWidth,box:panel.getBoundingClientRect().toJSON(),main:document.querySelector('.translation-main')!.getBoundingClientRect().toJSON()}});expect(layout.overflow).toBe(false);expect(layout.pageOverflow).toBe(false);expect(layout.box.right).toBeLessThanOrEqual(layout.main.left);
 await (await editableSource(page)).click();await expect(page.getByRole('button',{name:'Collapse vocabulary',exact:true})).toBeVisible();
 await page.screenshot({path:'artifacts/previews/2026-10-15-vocabulary-left-dark.png'});
 await page.keyboard.press('Escape');await expect(page.getByRole('button',{name:'Expand vocabulary',exact:true})).toBeFocused();await expect(page.locator('.vocabulary-side-card')).toHaveCount(0);expect(await requests(page)).toBe(0);
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='hide_window'))).toEqual([]);
 await page.keyboard.press('Escape');await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='hide_window').length)).toBe(1);
});

test('stream appends do not move a reader who has scrolled upward',async({page})=>{
 await fixture(page);await page.setViewportSize({width:460,height:650});await page.locator('.disclosure-control').click();
 const initial=Array.from({length:60},(_,i)=>`Paragraph ${i}: stable reading text.`).join('\n\n');
 await page.evaluate(translation=>{const w=window as any;Object.assign(w.task,{status:'translating',translation,revision:2});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})},initial);
 await expect(page.locator('.reading-block')).toHaveCount(60);await page.evaluate(()=>window.scrollTo(0,300));const before=await page.evaluate(()=>window.scrollY);expect(before).toBeGreaterThan(0);
 await page.evaluate(translation=>{const w=window as any;Object.assign(w.task,{translation:translation+'\n\nNew tail paragraph.',revision:3});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})},initial);
 await expect(page.locator('.reading-block')).toHaveCount(61);expect(Math.abs((await page.evaluate(()=>window.scrollY))-before)).toBeLessThanOrEqual(1);
});

test('a paused animation frame cannot leave completed translation blank',async({page})=>{
 await page.addInitScript(()=>{window.requestAnimationFrame=()=>1;window.cancelAnimationFrame=()=>{};});
 await fixture(page);await page.locator('.disclosure-control').click();
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{status:'completed',translation:'## Result\n- Visible even in a background window',revision:2});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});
 await expect(page.locator('.markdown-reading h2')).toHaveText('Result',{timeout:1500});
 await expect(page.locator('.markdown-reading li')).toHaveText('Visible even in a background window');
});


const vocabularyCards = (page: Page) => page.locator('.vocabulary-side-card');
const cardWords = (page: Page) => vocabularyCards(page).evaluateAll(cards => cards.map(card => card.getAttribute('data-word')));
const previewRequests = (page: Page) => page.evaluate(() => (window as any).calls.filter((call: any) => call.cmd === 'vocabulary_preview'));

async function selectionFixture(page: Page) {
  await fixture(page);
  await page.setViewportSize({width:680,height:800});
  await page.evaluate(() => {
    const w=window as any;
    const candidate=(word:string,meaning:string,phonetic='')=>({word,surface:word,phonetic,pos:word==='resilient'?'adj.':'n.',dict_meaning:meaning,context_meaning:meaning,context_sentence:'A resilient team adapts to change.',sentence_trans:'坚韧的团队适应变化。',difficulty:'B2'});
    Object.assign(w.task,{original_text:'A resilient team adapts to change.',status:'completed',translation:'坚韧的团队适应变化。',vocabulary:[candidate('resilient','坚韧的','/rɪˈzɪliənt/'),candidate('adapts','适应')],revision:2});
    w.previewResults={
      team:{candidate:candidate('team','团队','/tiːm/'),translation_highlights:[{start:3,end:5}]},
      change:{candidate:candidate('change','变化','/tʃeɪndʒ/'),translation_highlights:[{start:7,end:9}]},
      resilient:{candidate:candidate('resilient','坚韧的','/rɪˈzɪliənt/'),translation_highlights:[{start:0,end:3}]},
    };
    w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null});
  });
  await page.locator('.disclosure-control').click();
  await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();
  await expect(vocabularyCards(page)).toHaveCount(2);
}

async function selectSourceWord(page: Page, word: string) {
  await (await editableSource(page)).evaluate((el:HTMLTextAreaElement,selectedWord:string) => {
    const start=el.value.indexOf(selectedWord);
    if(start<0)throw new Error('Synthetic selected word was not found');
    el.focus();el.setSelectionRange(start,start+selectedWord.length);
    el.dispatchEvent(new Event('select',{bubbles:true}));
  },word);
}

for (const [source, language, word] of [
  ['Bonjour tout le monde.', 'fr', 'Bonjour'],
  ['这是包含 API 的中文原文。', 'zh-Hans', 'API'],
  ['Ambiguous', null, 'Ambiguous'],
] as const) {
  test(`non-English or unknown source ${language} does not offer vocabulary`, async ({ page }) => {
    await fixture(page);
    await page.evaluate(({ source, language }) => {
      const w = window as any;
      w.detectedLanguage = language;
      Object.assign(w.task, { original_text: source, detected_language: language, status: 'completed', translation: 'Synthetic translation.', revision: 2,
        vocabulary: [{word:'API',surface:'API',context_sentence:source,context_meaning:'Synthetic stale candidate'}] });
      w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null});
    }, { source, language });
    await page.locator('.disclosure-control').click();
    await selectSourceWord(page, word);
    // Cover both the preview debounce and the later language detection result.
    await page.waitForTimeout(450);
    await expect(page.locator('.vocabulary-sidebar')).toHaveCount(0);
    expect(await previewRequests(page)).toEqual([]);
    expect(await page.evaluate(() => (window as any).calls.filter((c:any) => c.cmd === 'vocabulary_collect'))).toEqual([]);
  });
}

test('translation clicks and selections never preview or collect vocabulary in either reading mode', async ({ page }) => {
  await selectionFixture(page);
  const before = await cardWords(page);
  for (const mode of ['Read', 'Plain text']) {
    await page.getByRole('button', {name:mode, exact:true}).click();
    const translation = page.locator('.translation-prose');
    await translation.click();
    await translation.evaluate(el => {
      const range = document.createRange();
      range.selectNodeContents(el);
      const selection = window.getSelection()!;
      selection.removeAllRanges();
      selection.addRange(range);
      document.dispatchEvent(new Event('selectionchange'));
      el.dispatchEvent(new MouseEvent('mouseup', {bubbles:true,button:0}));
    });
    await page.waitForTimeout(180);
    expect(await previewRequests(page)).toEqual([]);
    expect(await cardWords(page)).toEqual(before);
    expect(await page.evaluate(() => (window as any).calls.filter((c:any) => c.cmd === 'vocabulary_collect'))).toEqual([]);
  }
  await selectSourceWord(page, 'team');
  await expect.poll(() => previewRequests(page)).toHaveLength(1);
  await page.locator('.translation-prose').click();
  await page.waitForTimeout(180);
  expect(await previewRequests(page)).toHaveLength(1);
  expect(await page.evaluate(() => (window as any).calls.filter((c:any) => c.cmd === 'vocabulary_collect'))).toEqual([]);
});

test('an explicit English source selection enables preview when automatic detection is uncertain', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => {
    const w=window as any;
    w.detectedLanguage=null;
    Object.assign(w.task,{original_text:'Chipping away',detected_language:null,revision:2});
    w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null});
  });
  await page.getByLabel('Source language', {exact:true}).selectOption('en');
  await selectSourceWord(page,'Chipping away');
  await expect.poll(() => previewRequests(page)).toHaveLength(1);
  expect((await previewRequests(page))[0].args.sourceLanguage).toBe('en');
  expect(await page.evaluate(()=>(window as any).words)).toEqual([]);
});

test('changing the source language discards a pending English vocabulary preview', async ({ page }) => {
  await selectionFixture(page);
  await page.evaluate(() => { (window as any).deferredPreviewWords = ['team']; });
  await selectSourceWord(page, 'team');
  await expect.poll(() => previewRequests(page)).toHaveLength(1);
  await page.getByLabel('Source language', {exact:true}).selectOption('fr');
  await expect(page.locator('.vocabulary-sidebar')).toHaveCount(0);
  await page.evaluate(() => {
    const w=window as any;
    w.previewResolvers[0].resolve(w.previewResults.team);
  });
  await page.waitForTimeout(180);
  await expect(page.locator('.vocabulary-sidebar')).toHaveCount(0);
});

test('source selections preview at the top of the sidebar, deduplicate and save only on Add',async({page})=>{
  await selectionFixture(page);
  const sourceBox=await (await editableSource(page)).boundingBox();
  await selectSourceWord(page,'team');
  await expect.poll(()=>cardWords(page)).toEqual(['team','resilient','adapts']);
  await expect(page.getByRole('button',{name:'Add to vocabulary',exact:true})).toHaveCount(0);
  await expect(page.locator('.word-selection-bubble')).toHaveCount(0);
  expect(await page.evaluate(()=>(window as any).words)).toEqual([]);
  expect(await page.evaluate(()=>(window as any).calls.filter((call:any)=>['vocabulary_collect','vocabulary_manual'].includes(call.cmd)))).toEqual([]);
  expect((await previewRequests(page))[0].args).toEqual({sourceText:'A resilient team adapts to change.',selectionStart:12,selectionEnd:16,taskId:'fixture',attempt:1,sourceLanguage:null});
  await expect(vocabularyCards(page).first()).toHaveClass(/active/);
  await selectSourceWord(page,'resilient');
  await expect.poll(()=>cardWords(page)).toEqual(['team','resilient','adapts']);
  await expect(page.locator('.vocabulary-side-card[data-word="resilient"]')).toHaveClass(/active/);
  await selectSourceWord(page,'team');
  await expect.poll(()=>cardWords(page)).toEqual(['team','resilient','adapts']);
  expect((await previewRequests(page)).filter((call:any)=>call.args.selectionStart===12)).toHaveLength(1);
  await selectSourceWord(page,'change');
  await expect.poll(()=>cardWords(page)).toEqual(['change','team','resilient','adapts']);
  const sourceAfter=await (await editableSource(page)).boundingBox();
  expect(sourceAfter!.x).toBe(sourceBox!.x);expect(sourceAfter!.y).toBe(sourceBox!.y);
  expect(await requests(page)).toBe(0);
  const card=page.locator('.vocabulary-side-card[data-word="change"]');
  await card.getByRole('button',{name:'Add',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='vocabulary_collect').at(-1).args.sourceText)).toBe('A resilient team adapts to change.');
  await expect(card.getByRole('button',{name:'Saved (×1)',exact:true})).toBeDisabled();
  expect(await page.evaluate(()=>(window as any).words.map((word:any)=>word.word))).toEqual(['change']);
});

test('sidebar lookup supports more than five words and clears only current lookup failures',async({page})=>{
  await fixture(page);
  await page.setViewportSize({width:980,height:800});
  await page.evaluate(()=>{
    const w=window as any;
    const words=['substance','clarity','discipline','meaningful','continuity','recognition','reflection','discomfort','audience'];
    const source=words.join(' ')+'.';
    const candidate=(word:string)=>({word,surface:word,phonetic:'',pos:'n.',dict_meaning:'Synthetic local definition',context_meaning:'',context_sentence:source,sentence_trans:'',difficulty:''});
    w.words=words.slice(0,8).map(word=>({...candidate(word),id:'saved-'+word,status:'learning',review_count:1,contexts:[{...candidate(word),id:'context-'+word}]}));
    Object.assign(w.task,{status:'completed',original_text:source,translation:'Synthetic translation.',vocabulary:words.slice(0,5).map(candidate),revision:2});
    w.wire({type:'snapshot',snapshot:{revision:2,tasks:[w.task]},selected:w.task.id,notice:null});
  });
  await page.locator('.disclosure-control').click();
  await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();
  await expect(vocabularyCards(page)).toHaveCount(5);
  await expect(page.getByRole('button',{name:'Saved (×1)',exact:true})).toHaveCount(5);
  await page.evaluate(()=>{(window as any).lookupFailure='Synthetic lookup failure';});
  await selectSourceWord(page,'recognition');
  await expect(page.locator('.drawer-error')).toHaveText('Synthetic lookup failure');
  await page.evaluate(()=>{(window as any).lookupFailure='';});
  await selectSourceWord(page,'reflection');
  await expect(vocabularyCards(page)).toHaveCount(7);
  await expect(page.getByRole('button',{name:'Saved (×1)',exact:true})).toHaveCount(7);
  await expect(page.locator('.drawer-error')).toHaveCount(0);
  await page.evaluate(()=>{(window as any).deferLookup=true;});
  await selectSourceWord(page,'discomfort');
  await expect.poll(()=>page.evaluate(()=>(window as any).lookupResolvers.length)).toBe(1);
  await page.evaluate(()=>{(window as any).deferLookup=false;});
  await selectSourceWord(page,'audience');
  await expect(vocabularyCards(page)).toHaveCount(9);
  await expect(page.getByRole('button',{name:'Saved (×1)',exact:true})).toHaveCount(8);
  await page.evaluate(()=>{(window as any).lookupResolvers[0].reject('Stale lookup failure');});
  await expect(page.locator('.drawer-error')).toHaveCount(0);
  const last=page.locator('.vocabulary-side-card[data-word="audience"]');
  await last.getByRole('button',{name:'Add',exact:true}).click();
  await expect(last.getByRole('button',{name:'Saved (×1)',exact:true})).toBeDisabled();
  expect(new Set(await cardWords(page)).size).toBe(9);
  expect(await requests(page)).toBe(0);
  await page.screenshot({path:'artifacts/previews/vocabulary-without-count-cap.png'});
});

test('a newer source selection or task ignores a delayed preview from the previous selection',async({page})=>{
  await selectionFixture(page);
  await page.evaluate(()=>{(window as any).deferredPreviewWords=['team'];});
  await selectSourceWord(page,'team');
  await expect.poll(()=>previewRequests(page).then(calls=>calls.length)).toBe(1);
  await selectSourceWord(page,'change');
  await expect.poll(()=>cardWords(page)).toEqual(['change','resilient','adapts']);
  await page.evaluate(()=>{const w=window as any;w.previewResolvers.find((pending:any)=>pending.word==='team').resolve(w.previewResults.team);});
  await expect(page.locator('.vocabulary-side-card[data-word="team"]')).toHaveCount(0);
  await expect(vocabularyCards(page).first()).toHaveAttribute('data-word','change');
  await expect(page.locator('.translation-word-highlight')).toHaveText('变化');

  // A pending preview is also scoped to the source task and its attempt.
  await page.evaluate(()=>{const w=window as any;w.previewResolvers=[];w.deferredPreviewWords=['resilient'];});
  await selectSourceWord(page,'resilient');
  await expect.poll(()=>page.evaluate(()=>(window as any).previewResolvers.length)).toBe(1);
  await page.evaluate(()=>{
    const w=window as any;
    Object.assign(w.task,{id:'second-task',attempt:1,original_text:'A new source.',translation:'新的原文。',vocabulary:[],revision:3});
    w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:3},selected:w.task.id,notice:null});
    w.previewResolvers[0].resolve(w.previewResults.resilient);
  });
  await expect((await editableSource(page))).toHaveValue('A new source.');
  await expect(vocabularyCards(page)).toHaveCount(0);
  await expect(page.locator('.translation-word-highlight')).toHaveCount(0);
  expect(await requests(page)).toBe(0);
});

test('source meaning highlights work in Read and Plain text without changing copy or translating again',async({page})=>{
  await selectionFixture(page);
  const markdown='## Result\n\n坚韧的**团队**适应变化。';
  await page.evaluate(translation=>{
    const w=window as any;w.task.translation=translation;w.task.revision++;
    const start=translation.indexOf('团队');w.previewResults.team.translation_highlights=[{start,end:start+2}];
    const resilientStart=translation.indexOf('坚韧的');w.previewResults.resilient.translation_highlights=[{start:resilientStart,end:resilientStart+3}];
    w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]});
  },markdown);
  await selectSourceWord(page,'team');
  await expect(page.locator('.markdown-reading strong .translation-word-highlight')).toHaveText('团队');
  await expect(page.locator('.translation-word-highlight')).toHaveCount(1);
  await page.getByRole('button',{name:'Copy translation',exact:true}).click();
  const copied=await page.evaluate(()=>(window as any).calls.filter((call:any)=>call.cmd==='copy_text').at(-1).args.text);
  expect(copied).toContain('坚韧的团队适应变化。');expect(copied).not.toContain('<mark');
  await page.getByRole('button',{name:'Copy Markdown',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).calls.filter((call:any)=>call.cmd==='copy_text').at(-1).args.text)).toBe(markdown);
  await page.getByRole('button',{name:'Plain text',exact:true}).click();
  await expect(page.locator('.translation-word-highlight')).toHaveText('团队');
  await expect(page.locator('.translation-prose')).toHaveText(markdown);
  await selectSourceWord(page,'resilient');
  await expect(page.locator('.translation-word-highlight')).toHaveText('坚韧的');
  expect(await requests(page)).toBe(0);
});

test('sidebar pronunciation shows one trusted IPA and plays installed UK and US voices at both speeds',async({page})=>{
  await selectionFixture(page);
  const card=page.locator('.vocabulary-side-card[data-word="resilient"]');
  await expect(card.locator('.vocabulary-ipa')).toHaveText('/rɪˈzɪliənt/');
  for(const [accent,locale] of [['UK','en-GB'],['US','en-US']]){
    for(const slow of [false,true]){
      const label=slow?`Play resilient slowly (${accent})`:`Play resilient (${accent})`;
      await card.getByRole('button',{name:label,exact:true}).click();
      expect(await page.evaluate(()=>(window as any).calls.filter((call:any)=>call.cmd==='speak_word').at(-1).args)).toEqual({word:'resilient',locale,slow});
    }
  }
  expect(await requests(page)).toBe(0);
});

test('sidebar pronunciation disables an unavailable accent without claiming a second IPA',async({page})=>{
  await fixture(page);
  await page.evaluate(()=>{(window as any).speechVoices=[{locale:'en-GB',available:false},{locale:'en-US',available:true}];});
  await page.setViewportSize({width:680,height:800});
  await page.evaluate(()=>{
    const w=window as any;
    Object.assign(w.task,{status:'completed',translation:'坚韧的团队。',vocabulary:[{word:'resilient',surface:'resilient',phonetic:'/rɪˈzɪliənt/',pos:'adj.',dict_meaning:'坚韧的',context_meaning:'坚韧的',context_sentence:'A resilient team.',sentence_trans:'坚韧的团队。',difficulty:'B2'}],revision:2});
    w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null});
  });
  await page.locator('.disclosure-control').click();
  await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();
  const card=page.locator('.vocabulary-side-card[data-word="resilient"]');
  await expect(card.locator('.vocabulary-ipa')).toHaveCount(1);
  await expect(card.getByRole('button',{name:'Play resilient (UK)',exact:true})).toBeDisabled();
  await expect(card.getByRole('button',{name:'Play resilient slowly (UK)',exact:true})).toBeDisabled();
  await expect(card.getByRole('button',{name:'Play resilient (US)',exact:true})).toBeEnabled();
  await card.getByRole('button',{name:'Play resilient (US)',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).calls.filter((call:any)=>call.cmd==='speak_word').at(-1).args)).toEqual({word:'resilient',locale:'en-US',slow:false});
});


test('clicking inside a source word previews its complete token without changing the source',async({page})=>{
  await selectionFixture(page);
  const editor=(await editableSource(page));
  await editor.evaluate((el:HTMLTextAreaElement)=>{
    el.focus();el.setSelectionRange(14,14);
    el.dispatchEvent(new MouseEvent('mouseup',{bubbles:true,button:0}));
  });
  await expect.poll(()=>cardWords(page)).toEqual(['team','resilient','adapts']);
  await expect(editor).toHaveValue('A resilient team adapts to change.');
  expect((await previewRequests(page))[0].args).toEqual({sourceText:'A resilient team adapts to change.',selectionStart:12,selectionEnd:16,taskId:'fixture',attempt:1,sourceLanguage:null});
  expect(await requests(page)).toBe(0);
});


test('source selection opens an empty rail, respects manual collapse, and resets on a new attempt',async({page})=>{
  await selectionFixture(page);
  await page.evaluate(()=>{
    const w=window as any;w.task.vocabulary=[];w.task.revision++;
    w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]});
  });
  await expect(page.locator('.vocabulary-sidebar')).toHaveCount(0);
  await selectSourceWord(page,'team');
  await expect.poll(()=>cardWords(page)).toEqual(['team']);
  await page.getByRole('button',{name:'Collapse vocabulary',exact:true}).click();
  await expect(vocabularyCards(page)).toHaveCount(0);
  await selectSourceWord(page,'change');
  await expect(page.getByRole('button',{name:'Expand vocabulary',exact:true})).toBeVisible();
  await expect(vocabularyCards(page)).toHaveCount(0);
  await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();
  await expect.poll(()=>cardWords(page)).toEqual(['change','team']);
  await page.evaluate(()=>{
    const w=window as any;w.task.attempt++;w.task.revision++;
    w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]});
  });
  await expect(vocabularyCards(page)).toHaveCount(0);
  await expect(page.locator('.translation-word-highlight')).toHaveCount(0);
  await selectSourceWord(page,'team');
  await expect.poll(()=>cardWords(page)).toEqual(['team']);
  expect((await previewRequests(page)).at(-1).args.attempt).toBe(2);
  expect(await requests(page)).toBe(0);
});


test('selected word, pronunciation controls and meaning highlight remain readable at supported widths and in dark mode',async({page})=>{
  await selectionFixture(page);
  await page.emulateMedia({reducedMotion:'reduce'});
  await selectSourceWord(page,'team');
  for(const width of [580,680,980]){
    await page.setViewportSize({width,height:800});
    const card=page.locator('.vocabulary-side-card[data-word="team"]');
    await expect(card).toBeVisible();
    await expect(card.locator('.vocabulary-ipa')).toHaveText('/tiːm/');
    await expect(card.getByRole('button',{name:'Play team (UK)',exact:true})).toBeVisible();
    await expect(card.getByRole('button',{name:'Play team slowly (US)',exact:true})).toBeVisible();
    await expect(page.locator('.translation-word-highlight')).toHaveText('团队');
    await expect(page.locator('.translation-word-highlight')).toBeInViewport();
    const layout=await page.evaluate(()=>{
      const rail=document.querySelector('.vocabulary-sidebar')!,main=document.querySelector('.translation-main')!;
      return {overflow:document.documentElement.scrollWidth>innerWidth,railOverflow:rail.scrollWidth>rail.clientWidth,railRight:rail.getBoundingClientRect().right,mainLeft:main.getBoundingClientRect().left};
    });
    expect(layout.overflow).toBe(false);expect(layout.railOverflow).toBe(false);expect(layout.railRight).toBeLessThanOrEqual(layout.mainLeft);
    await page.screenshot({path:`artifacts/previews/2026-10-16-source-selection-${width}-light.png`});
  }
  await page.setViewportSize({width:680,height:800});
  await page.evaluate(()=>{const w=window as any;w.settings.theme='dark';w.emit('configuration-changed',w.settings);});
  await expect(page.locator('html')).toHaveAttribute('data-theme','dark');
  await expect(page.locator('.vocabulary-side-card[data-word="team"] .vocabulary-ipa')).toBeVisible();
  await expect(page.locator('.translation-word-highlight')).toBeVisible();
  await page.screenshot({path:'artifacts/previews/2026-10-16-source-selection-680-dark.png'});
  expect(await requests(page)).toBe(0);
});


test('late vocabulary metadata refreshes the selected meaning and highlight without reopening a collapsed sidebar',async({page})=>{
  await selectionFixture(page);
  await page.evaluate(()=>{
    const w=window as any;
    w.task.id='metadata-task';w.task.vocabulary=[];w.task.vocabulary_state='extracting';
    w.previewResults.team.candidate.context_meaning='';
    w.previewResults.team.candidate.dict_meaning='A group of people';
    w.previewResults.team.translation_highlights=[];
    w.task.revision++;
    w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id,notice:null});
  });
  await selectSourceWord(page,'team');
  await expect.poll(()=>cardWords(page)).toEqual(['team']);
  await expect(page.locator('.vocabulary-side-card[data-word="team"] .vocabulary-row-meaning')).toHaveText('A group of people');
  await expect(page.locator('.translation-word-highlight')).toHaveCount(0);
  await page.getByRole('button',{name:'Collapse vocabulary',exact:true}).click();
  await page.evaluate(()=>{
    const w=window as any;
    w.previewResults.team.candidate.context_meaning='团队';
    w.previewResults.team.translation_highlights=[{start:3,end:5}];
    w.task.vocabulary.push(structuredClone(w.previewResults.team.candidate));
    w.task.vocabulary_state='complete';w.task.revision++;
    w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]});
  });
  await expect.poll(()=>previewRequests(page).then(calls=>calls.length)).toBe(2);
  await expect(page.locator('.translation-word-highlight')).toHaveText('团队');
  await expect(page.getByRole('button',{name:'Expand vocabulary',exact:true})).toBeVisible();
  await expect(page.getByRole('button',{name:'Collapse vocabulary',exact:true})).toHaveCount(0);
  await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();
  await expect(page.locator('.vocabulary-side-card[data-word="team"]')).toHaveCount(1);
  await expect(page.locator('.vocabulary-side-card[data-word="team"] .vocabulary-row-meaning')).toHaveText('团队');
  expect(await requests(page)).toBe(0);
});


const dictionarySentence = 'A local dictionary explains individual words but cannot translate this complete example sentence.';
async function dictionaryFixture(page: Page, configured=true) {
  await fixture(page);
  await page.evaluate(({source,configured})=>{
    const w=window as any;
    const local={...w.settings.profiles[0],id:'dictionary',provider:'system_dictionary',name:'System dictionary',model:''};
    if(configured)w.settings.profiles[0].name='Configured translator';
    w.settings.profiles=configured?[w.settings.profiles[0],local]:[local];
    w.settings.active_profile='dictionary';
    w.guidanceResponses[source]={issue:'sentence',profile_ids:configured?['default']:[]};
    Object.assign(w.task,{profile:local,original_text:source,edited_text:null,status:'ready',error:null,translation:'',revision:w.task.revision+1});
    w.emit('configuration-changed',structuredClone(w.settings));
    w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id});
  },{source:dictionarySentence,configured});
  await expect(page.getByRole('region',{name:'Dictionary lookup guidance'})).toBeVisible();
}

test('dictionary guidance switches only this query and waits for explicit translation',async({page})=>{
  await dictionaryFixture(page);
  await expect(page.locator('.disclosure-control')).toBeDisabled();
  await expect(page.locator('.capture-hint')).toHaveCount(0);
  await expect(page.getByText('Enable the dictionaries you need',{exact:false})).toHaveCount(0);
  expect(await requests(page)).toBe(0);
  await page.getByRole('combobox',{name:'Translation service',exact:true}).selectOption('default');
  expect(await requests(page)).toBe(0);
  await page.getByRole('button',{name:'Use this service',exact:true}).click();
  await expect(page.locator('.provider-name')).toHaveText('Configured translator');
  await expect(page.getByRole('textbox',{name:'Source reading',exact:true})).toHaveText(dictionarySentence);
  await expect(page.getByRole('combobox',{name:'Translation language',exact:true})).toBeVisible();
  await expect(page.locator('.disclosure-control')).toBeFocused();
  expect(await requests(page)).toBe(0);
  expect(await page.evaluate(()=>(window as any).settings.active_profile)).toBe('dictionary');
  await page.keyboard.press('d');
  expect(await requests(page)).toBe(1);
  expect(await page.evaluate(()=>(window as any).calls.find((c:any)=>c.cmd==='retry_task').args.profileId)).toBe('default');
  await page.evaluate(()=>(window as any).complete());
  await expect(page.locator('.provider-name')).toHaveText('Configured translator');
});

test('dictionary guidance offers settings without dispatching a lookup when no service is configured',async({page})=>{
  await dictionaryFixture(page,false);
  await expect(page.getByRole('combobox',{name:'Translation service',exact:true})).toHaveCount(0);
  await expect(page.getByText('No translation service can be selected.',{exact:false})).toBeVisible();
  await page.keyboard.press('d');
  expect(await requests(page)).toBe(0);
  await page.getByRole('button',{name:'Open settings',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='show_settings').length)).toBe(1);
  expect(await requests(page)).toBe(0);
});

test('dictionary miss and stale guidance recover when the query changes',async({page})=>{
  await dictionaryFixture(page);
  await page.evaluate(()=>{
    const w=window as any;
    Object.assign(w.task,{original_text:'unmatched',status:'failed',error:'本机词典未找到释义，请在 macOS 词典中启用所需词典。',revision:w.task.revision+1});
    w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id});
  });
  await expect(page.getByRole('region',{name:'Dictionary lookup guidance'})).toContainText('No matching dictionary entry was found.');
  await expect(page.getByText('No definition yet. Try again.',{exact:true})).toHaveCount(0);
  await (await editableSource(page)).fill('hello');
  await expect(page.getByRole('region',{name:'Dictionary lookup guidance'})).toHaveCount(0);
  await expect(page.locator('.disclosure-control')).toBeEnabled();
  await page.evaluate(()=>(window as any).deferGuidance=true);
  await (await editableSource(page)).fill(dictionarySentence);
  await expect.poll(()=>page.evaluate(()=>(window as any).guidanceResolvers.length)).toBe(1);
  await (await editableSource(page)).fill('good morning');
  await expect.poll(()=>page.evaluate(()=>(window as any).guidanceResolvers.length)).toBe(2);
  await page.evaluate(()=>{const w=window as any;w.guidanceResolvers[1].resolve({issue:null,profile_ids:[]});});
  await expect(page.locator('.disclosure-control')).toBeEnabled();
  await page.evaluate(()=>{const w=window as any;w.guidanceResolvers[0].resolve({issue:'sentence',profile_ids:['default']});});
  await expect(page.getByRole('region',{name:'Dictionary lookup guidance'})).toHaveCount(0);
  await expect(page.locator('.disclosure-control')).toBeEnabled();
  expect(await requests(page)).toBe(0);
});

test('dictionary guidance is localized and readable in narrow dark windows',async({page})=>{
  await dictionaryFixture(page);
  await page.emulateMedia({reducedMotion:'reduce'});
  for(const theme of ['light','dark']){
    await page.evaluate(theme=>{const w=window as any;w.settings.theme=theme;w.settings.ui_language='zh';w.emit('configuration-changed',structuredClone(w.settings));},theme);
    for(const width of [360,580,980]){
      await page.setViewportSize({width,height:800});
      await expect(page.getByRole('region',{name:'词典查询提示'})).toContainText('整句或段落请使用翻译服务');
      await expect(page.getByRole('button',{name:'使用此服务',exact:true})).toBeVisible();
      expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
    }
  }
  await page.setViewportSize({width:580,height:800});
  await page.screenshot({path:'artifacts/dictionary-guidance/after-dark.png'});
  await page.evaluate(()=>{const w=window as any;w.settings.theme='light';w.settings.ui_language='en';w.emit('configuration-changed',structuredClone(w.settings));});
  await page.screenshot({path:'artifacts/dictionary-guidance/after-light.png'});
});


test('dictionary alternatives work for a new manual draft and are cleared when a service is removed',async({page})=>{
  await fixture(page,false);
  await page.evaluate(source=>{
    const w=window as any;
    w.settings.profiles[0].name='Configured translator';
    w.settings.profiles.push({...w.settings.profiles[0],id:'dictionary',provider:'system_dictionary',name:'System dictionary',model:''});
    w.settings.active_profile='dictionary';
    w.guidanceResponses[source]={issue:'sentence',profile_ids:['default']};
    w.emit('configuration-changed',structuredClone(w.settings));
  },dictionarySentence);
  await (await editableSource(page)).fill(dictionarySentence);
  await page.getByRole('button',{name:'Use this service',exact:true}).click();
  expect(await requests(page)).toBe(0);
  await page.keyboard.press('d');
  expect(await requests(page)).toBe(1);
  expect(await page.evaluate(()=>(window as any).calls.find((c:any)=>c.cmd==='translate_text').args.profileId)).toBe('default');
  await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{status:'completed',translation:'Synthetic translation',revision:w.task.revision+1});w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id});});
  await expect(page.locator('.provider-name')).toHaveText('Configured translator');
  await page.evaluate(()=>{const w=window as any;w.task.id='restored-query';w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id});});
  await expect(page.locator('.provider-name')).toHaveText('Configured translator');
  await expect(page.getByRole('region',{name:'Translation',exact:true})).toContainText('Synthetic translation');
  expect(await page.evaluate(()=>(window as any).settings.active_profile)).toBe('dictionary');
  await page.evaluate(()=>{const w=window as any;w.settings.profiles=w.settings.profiles.filter((p:any)=>p.id==='dictionary');w.guidanceResponses[w.task.original_text]={issue:'sentence',profile_ids:[]};w.emit('configuration-changed',structuredClone(w.settings));});
  await expect(page.locator('.provider-name')).toHaveText('System dictionary');
  await expect(page.getByRole('button',{name:'Open settings',exact:true})).toBeVisible();
  expect(await requests(page)).toBe(1);
});

test('manual query service and languages survive focus and appearance refreshes',async({page})=>{
  await fixture(page,false,'en');
  await expect(page.getByRole('combobox',{name:'Source language',exact:true})).toHaveValue('en');
  await page.evaluate(source=>{
    const w=window as any;
    w.settings.profiles[0].name='Chosen translator';
    w.settings.profiles.push({...w.settings.profiles[0],id:'dictionary',provider:'system_dictionary',name:'System dictionary',model:''});
    w.settings.active_profile='dictionary';
    w.guidanceResponses[source]={issue:'sentence',profile_ids:['default']};
    w.emit('configuration-changed',structuredClone(w.settings));
  },dictionarySentence);
  await (await editableSource(page)).fill(dictionarySentence);
  await page.getByRole('button',{name:'Use this service',exact:true}).click();
  await page.getByRole('combobox',{name:'Source language',exact:true}).selectOption('fr');
  await page.getByRole('combobox',{name:'Translation language',exact:true}).selectOption('de');
  await page.evaluate(()=>window.dispatchEvent(new Event('focus')));
  await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='configuration_settings').length)).toBeGreaterThan(0);
  await page.evaluate(()=>{const w=window as any;w.settings.theme='dark';w.emit('configuration-changed',structuredClone(w.settings));});
  await expect(page.locator('.provider-name')).toHaveText('Chosen translator');
  await expect(page.getByRole('combobox',{name:'Source language',exact:true})).toHaveValue('fr');
  await expect(page.getByRole('combobox',{name:'Translation language',exact:true})).toHaveValue('de');
  await expect((await editableSource(page))).toHaveValue(dictionarySentence);
  expect(await requests(page)).toBe(0);
  await page.locator('.disclosure-control').click();
  expect(await requests(page)).toBe(1);
  expect(await page.evaluate(()=>(window as any).calls.find((c:any)=>c.cmd==='translate_text').args)).toMatchObject({profileId:'default',sourceLanguage:'fr',targetLanguage:'de'});
  await page.evaluate(()=>{
    const w=window as any;
    Object.assign(w.task,{id:'next-capture',profile:structuredClone(w.settings.profiles[1]),status:'ready',original_text:'hello',edited_text:null,source_language:null,revision:w.task.revision+1});
    w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id});
  });
  await expect(page.locator('.provider-name')).toHaveText('System dictionary');
  await expect(page.getByRole('combobox',{name:'Source language',exact:true})).toHaveValue('en');
  expect(await requests(page)).toBe(1);
});

test('dictionary alternatives are hidden for sensitive content',async({page})=>{
  await dictionaryFixture(page);
  await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{original_text:'',edited_text:null,sensitive:'API_SECRET',revision:w.task.revision+1});w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id});});
  await expect(page.getByRole('region',{name:'Dictionary lookup guidance'})).toHaveCount(0);
  await expect(page.getByRole('button',{name:'Use this service',exact:true})).toHaveCount(0);
  expect(await requests(page)).toBe(0);
});

test('sensitive confirmation keeps the displayed query service when the default changes',async({page})=>{
  await fixture(page);
  await page.evaluate(()=>{
    const w=window as any;
    w.settings.profiles.push({...w.settings.profiles[0],id:'another',name:'Another service'});
    Object.assign(w.task,{source:'manual',sensitive:'API_SECRET',original_text:'',edited_text:null,revision:w.task.revision+1});
    w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id});
    w.settings.active_profile='another';
    w.emit('configuration-changed',structuredClone(w.settings));
  });
  await expect(page.getByRole('button',{name:'Reveal source and translate',exact:true})).toBeVisible();
  await expect(page.locator('.provider-name')).toHaveText('Custom service');
  expect(await requests(page)).toBe(0);
  await page.getByRole('button',{name:'Reveal source and translate',exact:true}).click();
  await page.evaluate(()=>{
    const w=window as any;
    Object.assign(w.task,{sensitive:null,status:'completed',original_text:'Synthetic confirmed source.',translation:'Synthetic confirmed translation.',revision:w.task.revision+1});
    w.wire({type:'snapshot',snapshot:{tasks:[structuredClone(w.task)],revision:w.task.revision},selected:w.task.id});
  });
  await expect(page.locator('.provider-name')).toHaveText('Custom service');
  await expect(page.getByRole('region',{name:'Translation',exact:true})).toContainText('Synthetic confirmed translation.');
  expect(await page.evaluate(()=>(window as any).settings.active_profile)).toBe('another');
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='resolve_sensitive').length)).toBe(1);
});

test('restoring the native window refits unchanged compact content', async ({ page }) => {
  await fixture(page);
  await expect.poll(() => page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'fit_result_height').length)).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Expand', exact: true }).click();
  await page.setViewportSize({ width: 1000, height: 800 });
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  // macOS delivers the restored size before the final mode notification.
  await page.setViewportSize({ width: 460, height: 560 });
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const fits = await page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'fit_result_height').length);
  await page.evaluate(() => (window as any).emit('window-mode-changed', false));
  await expect.poll(() => page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'fit_result_height').length)).toBeGreaterThan(fits);
  const height = await page.locator('.translation-content').evaluate(el => Math.ceil(el.getBoundingClientRect().height + 44));
  expect(await page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'fit_result_height').at(-1).args.height)).toBe(height);
});

test('restoring content height still works when WebKit pauses animation frames', async ({page}) => {
  await fixture(page);
  await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='fit_result_height').length)).toBeGreaterThan(0);
  const before = await page.evaluate(()=>{
    const w=window as any;
    w.requestAnimationFrame=()=>0;
    w.emit('window-mode-changed',true);
    return w.calls.filter((c:any)=>c.cmd==='fit_result_height').length;
  });
  await page.evaluate(()=>(window as any).emit('window-mode-changed',false));
  await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='fit_result_height').length)).toBeGreaterThan(before);
});
