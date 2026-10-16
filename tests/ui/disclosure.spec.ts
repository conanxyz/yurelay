import { test, expect, type Page } from '@playwright/test';

async function fixture(page: Page, initiallySelected=true) {
  await page.addInitScript((initialSelection) => {
    const w = window as any;
    w.isTauri = true;
    w.calls = [];w.words=[];
    w.speechVoices = [{locale:'en-GB',available:true},{locale:'en-US',available:true}];
    w.previewResults = {};w.deferredPreviewWords=[];w.previewResolvers=[];
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    const callbacks: Record<number, Function> = {}, events: Record<string, number> = {};
    let callback = 0;
    w.settings = {
      profiles: [{ id: 'default', provider: 'custom', name: 'Custom service', model: 'gpt-6-luna', revision: 1 }],
      active_profile: 'default', target_language: 'zh-Hans', ui_language: 'en',
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
        if(cmd==='vocabulary_list'||cmd==='vocabulary_lookup')return structuredClone(w.words);
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
        if (cmd === 'detect_text_language') return 'en';
        if (cmd === 'subscribe') { w.wire = (m: unknown) => args.onEvent.onmessage(m); return 'fixture'; }
        if (cmd === 'retry_task') {
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
  },initiallySelected);
  await page.goto('/');
  if(initiallySelected)await expect(page.locator('.disclosure-control')).toBeEnabled();else await expect(page.locator('.disclosure-control')).toBeDisabled();
}

const requests = (page: Page) => page.evaluate(() => (window as any).calls.filter((c: any) => c.cmd === 'retry_task').length);
const complete = (page: Page) => page.evaluate(() => (window as any).complete());

test('arrow reflects state; X and button toggle, retain result and detect edits', async ({ page }) => {
  await fixture(page);
  const toggle = page.locator('.disclosure-control');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  // The current state uses a left chevron when collapsed and down when open.
  await expect(toggle.locator('svg')).toHaveCSS('transform', 'matrix(-1, 0, 0, -1, 0, 0)');
  await toggle.click();
  await expect.poll(() => requests(page)).toBe(1);
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await expect(toggle.locator('svg')).toHaveCSS('transform', 'matrix(0, 1, -1, 0, 0, 0)');
  await page.keyboard.press('x');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await complete(page);
  await expect(page.locator('#translation-body')).toBeHidden();
  await page.keyboard.press('x');
  await expect(page.locator('.translation-prose')).toHaveText('模型当前已满，请尝试其他模型。');
  await toggle.click();
  await toggle.click();
  expect(await requests(page)).toBe(1);
  const editor = page.getByRole('textbox', { name: 'Source text' });
  await editor.click();
  await page.keyboard.press('End');
  await page.keyboard.press('x');
  expect(await editor.inputValue()).toContain('x');
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
  await page.keyboard.press('x');
  expect(await requests(page)).toBe(0);
  const editor = page.getByRole('textbox', { name: 'Source text' });
  await editor.click();
  await page.evaluate(() => { window.dispatchEvent(new Event('blur')); window.dispatchEvent(new Event('focus')); });
  await page.keyboard.press('x');
  expect(await editor.inputValue()).toContain('x');
  expect(await requests(page)).toBe(0);
  await page.locator('.window-name').click();
  await page.keyboard.press('x');
  expect(await requests(page)).toBe(1);
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
  await page.keyboard.press('x');
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
  await expect(page.getByRole('textbox', { name: 'Source text' })).toHaveValue('');
  await expect(page.locator('.translation-prose')).toHaveCount(0);
  for (const source of ['The second selection.', 'The third selection.']) {
    await page.evaluate((text) => {
      const w = window as any;
      const task = { ...w.task, id: text, status: 'ready', original_text: text, edited_text: null, translation: '', revision: 5 };
      w.wire({ type: 'snapshot', snapshot: { revision: 5, tasks: [w.task, task] }, selected: task.id, notice: null });
    }, source);
    await expect(page.getByRole('textbox', { name: 'Source text' })).toHaveValue(source);
    await expect(page.locator('#translation-body')).toHaveCount(0);
  }
});


test('a snapshot without an explicit selection starts blank and history opens only on selection', async ({page}) => {
  await fixture(page);
  await page.evaluate(()=>{const w=window as any;w.complete();w.wire({type:'snapshot',snapshot:{revision:5,tasks:[w.task]},selected:null,notice:null});});
  await expect(page.getByRole('textbox',{name:'Source text'})).toHaveValue('');
  await expect(page.locator('.translation-prose')).toHaveCount(0);
  await expect(page.locator('.window-chrome img')).toHaveCount(0);
  await page.evaluate(()=>{const w=window as any;w.wire({type:'snapshot',snapshot:{revision:5,tasks:[w.task]},selected:w.task.id,notice:null});});
  await expect(page.locator('.translation-prose')).toHaveText('模型当前已满，请尝试其他模型。');
});


test('cold bootstrap keeps saved history out of the translation window', async ({page}) => {
  await fixture(page,false);
  await expect(page.getByRole('textbox',{name:'Source text'})).toHaveValue('');
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
  await expect(page.locator('.disclosure-control')).toHaveAttribute('aria-label','Look up and expand (X)');
});

test('sensitive snapshot shows only the mask; X cannot bypass explicit consent',async({page})=>{
 await fixture(page);
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{sensitive:'MNEMONIC',original_text:'',translation:'',revision:2});w.wire({type:'snapshot',snapshot:{tasks:[w.task],revision:2},selected:w.task.id,notice:null})});
 await expect(page.getByRole('textbox',{name:'Source text'})).toHaveCount(0);
 await expect(page.getByRole('alert')).toContainText('Possible sensitive information');await page.keyboard.press('x');expect(await requests(page)).toBe(0);
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


test('vocabulary automatically opens left once and preserves manual collapse for the same attempt',async({page})=>{
 await fixture(page);await page.setViewportSize({width:460,height:800});await page.locator('.disclosure-control').click();
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{status:'translating',extract_vocabulary:true,vocabulary_state:'none',translation:'译文',revision:2});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});
 await expect(page.getByRole('button',{name:'Copy translation',exact:true})).toBeDisabled();await expect(page.locator('.vocabulary-sidebar')).toHaveCount(0);
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{status:'completed',vocabulary_state:'extracting',revision:3});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});
 await expect(page.getByRole('button',{name:'Copy translation',exact:true})).toBeEnabled();await expect(page.locator('.translation-prose')).toHaveText('译文');const before=await page.locator('.translation-main').boundingBox();
 await page.evaluate(()=>{const w=window as any;Object.assign(w.task,{vocabulary_state:'complete',vocabulary:Array.from({length:5},(_,i)=>({word:'word'+i,surface:'word'+i,phonetic:'',pos:'n.',context_meaning:'语境释义',context_sentence:'Sentence '+i,dict_meaning:'',sentence_trans:'',difficulty:'B2'})),revision:4});w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});
 await page.setViewportSize({width:680,height:800});await expect(page.getByRole('button',{name:'Collapse vocabulary',exact:true})).toBeVisible();await expect(page.locator('.vocabulary-side-card')).toHaveCount(5);
 const after=await page.locator('.translation-main').boundingBox();expect(after!.width).toBe(before!.width);expect(after!.x).toBe(220);expect(Math.abs(after!.height-before!.height)).toBeLessThan(1);
 await page.getByRole('button',{name:'Collapse vocabulary',exact:true}).click();await expect(page.locator('.vocabulary-sidebar')).toHaveClass(/collapsed/);await expect(page.locator('.vocabulary-side-card')).toHaveCount(0);
 await page.evaluate(()=>{const w=window as any;w.task.revision++;w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});await expect(page.getByRole('button',{name:'Expand vocabulary',exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Expand vocabulary',exact:true}).click();await page.getByRole('button',{name:'View word0 meaning',exact:true}).click();await expect(page.locator('.vocabulary-row-detail')).toContainText('Sentence 0');
 await page.screenshot({path:'artifacts/previews/2026-10-15-vocabulary-left.png'});
 await page.getByRole('button',{name:'Collapse vocabulary',exact:true}).click();await page.evaluate(()=>{const w=window as any;w.task.attempt++;w.task.revision++;w.wire({type:'events',events:[{type:'changed',task:structuredClone(w.task)}]})});await expect(page.getByRole('button',{name:'Collapse vocabulary',exact:true})).toBeVisible();
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
 await page.locator('.disclosure-control').click();await expect(page.getByRole('button',{name:'Collapse vocabulary',exact:true})).toBeVisible();await expect(page.locator('.vocabulary-ipa')).toHaveCount(0);
 await page.getByRole('button',{name:'View OAuth2 meaning',exact:true}).click();await expect(page.locator('.vocabulary-row-detail')).toContainText('通用授权框架');
 const layout=await page.evaluate(()=>{const panel=document.querySelector('.vocabulary-sidebar')!;return {overflow:panel.scrollWidth>panel.clientWidth,pageOverflow:document.documentElement.scrollWidth>innerWidth,box:panel.getBoundingClientRect().toJSON(),main:document.querySelector('.translation-main')!.getBoundingClientRect().toJSON()}});expect(layout.overflow).toBe(false);expect(layout.pageOverflow).toBe(false);expect(layout.box.right).toBeLessThanOrEqual(layout.main.left);
 await page.getByRole('textbox',{name:'Source text'}).click();await expect(page.getByRole('button',{name:'Collapse vocabulary',exact:true})).toBeVisible();
 await page.screenshot({path:'artifacts/previews/2026-10-15-vocabulary-left-dark.png'});
 await page.keyboard.press('Escape');await expect(page.getByRole('button',{name:'Expand vocabulary',exact:true})).toBeFocused();await expect(page.locator('.vocabulary-side-card')).toHaveCount(0);expect(await requests(page)).toBe(0);
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
  await expect(vocabularyCards(page)).toHaveCount(2);
}

async function selectSourceWord(page: Page, word: string) {
  await page.getByRole('textbox',{name:'Source text'}).evaluate((el:HTMLTextAreaElement,selectedWord:string) => {
    const start=el.value.indexOf(selectedWord);
    if(start<0)throw new Error('Synthetic selected word was not found');
    el.focus();el.setSelectionRange(start,start+selectedWord.length);
    el.dispatchEvent(new Event('select',{bubbles:true}));
  },word);
}

test('source selections preview at the top of the sidebar, deduplicate and save only on Add',async({page})=>{
  await selectionFixture(page);
  const sourceBox=await page.getByRole('textbox',{name:'Source text'}).boundingBox();
  await selectSourceWord(page,'team');
  await expect.poll(()=>cardWords(page)).toEqual(['team','resilient','adapts']);
  await expect(page.getByRole('button',{name:'Add to vocabulary',exact:true})).toHaveCount(0);
  await expect(page.locator('.word-selection-bubble')).toHaveCount(0);
  expect(await page.evaluate(()=>(window as any).words)).toEqual([]);
  expect(await page.evaluate(()=>(window as any).calls.filter((call:any)=>['vocabulary_collect','vocabulary_manual'].includes(call.cmd)))).toEqual([]);
  expect((await previewRequests(page))[0].args).toEqual({sourceText:'A resilient team adapts to change.',selectionStart:12,selectionEnd:16,taskId:'fixture',attempt:1});
  await expect(vocabularyCards(page).first()).toHaveClass(/active/);
  await selectSourceWord(page,'resilient');
  await expect.poll(()=>cardWords(page)).toEqual(['team','resilient','adapts']);
  await expect(page.locator('.vocabulary-side-card[data-word="resilient"]')).toHaveClass(/active/);
  await selectSourceWord(page,'team');
  await expect.poll(()=>cardWords(page)).toEqual(['team','resilient','adapts']);
  expect((await previewRequests(page)).filter((call:any)=>call.args.selectionStart===12)).toHaveLength(1);
  await selectSourceWord(page,'change');
  await expect.poll(()=>cardWords(page)).toEqual(['change','team','resilient','adapts']);
  const sourceAfter=await page.getByRole('textbox',{name:'Source text'}).boundingBox();
  expect(sourceAfter!.x).toBe(sourceBox!.x);expect(sourceAfter!.y).toBe(sourceBox!.y);
  expect(await requests(page)).toBe(0);
  const card=page.locator('.vocabulary-side-card[data-word="change"]');
  await card.getByRole('button',{name:'Add',exact:true}).click();
  await expect(card.getByRole('button',{name:'Saved (×1)',exact:true})).toBeDisabled();
  expect(await page.evaluate(()=>(window as any).words.map((word:any)=>word.word))).toEqual(['change']);
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
  await expect(page.getByRole('textbox',{name:'Source text'})).toHaveValue('A new source.');
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
  const editor=page.getByRole('textbox',{name:'Source text'});
  await editor.evaluate((el:HTMLTextAreaElement)=>{
    el.focus();el.setSelectionRange(14,14);
    el.dispatchEvent(new MouseEvent('mouseup',{bubbles:true,button:0}));
  });
  await expect.poll(()=>cardWords(page)).toEqual(['team','resilient','adapts']);
  await expect(editor).toHaveValue('A resilient team adapts to change.');
  expect((await previewRequests(page))[0].args).toEqual({sourceText:'A resilient team adapts to change.',selectionStart:12,selectionEnd:16,taskId:'fixture',attempt:1});
  expect(await requests(page)).toBe(0);
});


test('an explicit source selection opens an empty or collapsed rail and a new attempt clears its preview',async({page})=>{
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
