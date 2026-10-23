import { test, expect, type Page } from '@playwright/test';
import { defaults } from '../../src/types';

async function setup(page: Page, options: { legacy?: boolean; configured?: boolean; locale?: string } = {}) {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.addInitScript(({ settings, options }) => {
    const w = window as any;
    w.isTauri = true; w.calls = [];
    w.settings = structuredClone(settings); w.settings.ui_language = options.locale ?? 'en';
    if (options.configured) Object.assign(w.settings.profiles[0], { provider: 'system_dictionary', name: 'System dictionary' });
    w.permissions = { accessibility: false, screen: false };
    w.progress = JSON.parse(localStorage.getItem('fixture-progress') ?? 'null') ?? { version: 1, setup: options.legacy ? 'legacy' : 'new', step: 0, tutorial: 'new', lesson: 0 };
    let id = 0;
    const callbacks: Record<number, Function> = {}, events: Record<string, number[]> = {};
    w.emit = (event: string, payload: unknown) => events[event]?.forEach(key=>callbacks[key]?.({ event, payload }));
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback: (fn: Function) => { callbacks[++id] = fn; return id; }, unregisterCallback() {},
      invoke: async (cmd: string, args: any) => {
        w.calls.push({ cmd, args });
        if (cmd === 'plugin:event|listen') { (events[args.event] ??= []).push(args.handler); return id; }
        if (cmd === 'bootstrap') return { configuration: { settings: w.settings, has_keys: [], credential_errors: {} }, permissions: w.permissions, shortcuts: { registered: true, error: null }, snapshot: { revision: 0, tasks: [] }, selected: null, notice: null, expanded: false, pinned: false };
        if (cmd === 'configuration_settings') return structuredClone(w.settings);
        if (cmd === 'permissions') return structuredClone(w.permissions);
        if (cmd === 'experience_status') return { progress: structuredClone(w.progress), services: [{profile_id:'default',state:options.configured?'configured':'needs_configuration',dictionary:!!options.configured,translation:!options.configured,generation:!options.configured}], shortcuts: { registered: !w.shortcutConflict, error: w.shortcutConflict ? 'synthetic conflict' : null } };
        if (cmd === 'update_experience') {
          if(w.failProgress)throw 'Synthetic progress save failed';
          if(w.deferProgress)await new Promise(resolve=>w.releaseProgress=resolve);
          const a = args.action;
          if(a.type === 'step') { w.progress.step = a.step; if(w.progress.setup === 'new')w.progress.setup='started'; }
          if(a.type === 'lesson') { w.progress.lesson = a.lesson; w.progress.tutorial='started'; }
          if(a.type === 'finish_tutorial')w.progress.tutorial='completed';
          if(a.type === 'skip_tutorial')w.progress.tutorial='skipped';
          if(a.type === 'skip')w.progress.setup='skipped';
          if(a.type === 'finish')w.progress.setup='completed';
          localStorage.setItem('fixture-progress',JSON.stringify(w.progress));
          w.emit('experience-changed',{progress:structuredClone(w.progress),close_guide:['skip','finish'].includes(a.type)});
          return structuredClone(w.progress);
        }
        if(cmd === 'service_verifications')return [];
        if(cmd === 'subscribe')return 'fixture';
        if(cmd === 'dictionary_guidance')return {issue:null,profile_ids:[]};
        if(cmd === 'detect_text_language')return 'en';
        if(cmd === 'vocabulary_lookup' || cmd === 'operation_list')return [];
        return null;
      }
    };
  }, { settings: defaults, options });
  await page.goto('/');
  if(!options.legacy)await expect(page.getByRole('region',{name:'Getting started'})).toBeVisible();
}
const networkActions = (page: Page) => page.evaluate(()=>(window as any).calls.filter((c:any)=>['translate_text','retry_task','test_connection','start_capture','copy_text','save_configuration'].includes(c.cmd)));

test('first run requests each permission only on click and refreshes actual status', async ({page})=>{
  await setup(page);
  await expect(page.getByText('Not allowed',{exact:true})).toHaveCount(2);
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='request_permission'))).toEqual([]);
  await page.getByRole('button',{name:'Open system settings · Accessibility',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='request_permission').map((c:any)=>c.args))).toEqual([{kind:'accessibility'}]);
  await expect(page.getByText('Not allowed',{exact:true})).toHaveCount(2);
  await page.evaluate(()=>{(window as any).permissions.accessibility=true;window.dispatchEvent(new Event('focus'));});
  await expect(page.getByText('Allowed',{exact:true})).toHaveCount(1);
  await page.getByRole('button',{name:'Open system settings · Screen recording',exact:true}).click();
  await expect(page.getByText('Not allowed',{exact:true})).toHaveCount(1);
  expect(await networkActions(page)).toEqual([]);
});

test('skip is persistent and manual input remains available with no permissions',async({page})=>{
  await setup(page);
  await page.getByRole('button',{name:'Set up later',exact:true}).click();
  await page.getByRole('textbox',{name:'Source text',exact:true}).fill('A synthetic sentence.');
  await expect(page.getByRole('button',{name:'Set up a service',exact:true})).toBeVisible();
  await expect(page.locator('.disclosure-control')).toBeDisabled();
  await page.reload();
  await expect(page.getByRole('region',{name:'Getting started'})).toHaveCount(0);
  await expect(page.getByRole('button',{name:'Settings',exact:true})).toBeVisible();
  expect(await networkActions(page)).toEqual([]);
});

test('service choice navigates to explicit setup and resumes after restart',async({page})=>{
  await setup(page);
  await page.getByRole('button',{name:'Continue to services',exact:true}).click();
  await page.reload();
  await expect(page.getByRole('heading',{name:'Choose a service before your first query'})).toBeVisible();
  await page.getByRole('button',{name:'Choose system dictionary',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='open_settings_at').at(-1).args.route)).toEqual({section:'services',profile_id:null,dictionary:true});
  await page.getByRole('button',{name:'Configure translation service',exact:true}).click();
  expect(await networkActions(page)).toEqual([]);
});

test('tutorial is independent of setup, uses current shortcuts and preserves the draft',async({page})=>{
  await setup(page,{legacy:true,configured:true});
  await expect(page.getByRole('region',{name:'Getting started'})).toHaveCount(0);
  await page.getByRole('textbox',{name:'Source text',exact:true}).fill('My synthetic draft');
  await page.evaluate(()=>{const w=window as any;w.settings.selection_shortcut='Control+Shift+L';w.emit('configuration-changed',structuredClone(w.settings));});
  await page.getByRole('button',{name:'Usage guide',exact:true}).click();
  await page.getByRole('button',{name:/3\s*Practice/}).click();
  await expect(page.locator('.guide-basics')).toContainText('⌃⇧L');
  await page.keyboard.press('s');
  await expect(page.locator('.demo-result')).toBeVisible();
  await page.getByRole('button',{name:'Start using Yurelay',exact:true}).click();
  await expect(page.getByRole('textbox',{name:'Source text',exact:true})).toHaveValue('My synthetic draft');
  expect(await networkActions(page)).toEqual([]);
  expect(await page.evaluate(()=>(window as any).progress.tutorial)).toBe('completed');
});

test('saved service entry and settings are keyboard accessible at narrow widths',async({page})=>{
  await setup(page,{legacy:true,configured:true});
  await page.setViewportSize({width:360,height:540});
  const settings=page.getByRole('button',{name:'Settings',exact:true});
  await settings.focus(); await page.keyboard.press('Enter');
  expect(await page.evaluate(()=>(window as any).calls.some((c:any)=>c.cmd==='show_settings'))).toBe(true);
  const service=page.getByRole('button',{name:'Change service or model',exact:true});
  await service.focus();await page.keyboard.press('Enter');
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='open_settings_at').at(-1).args.route)).toEqual({section:'services',profile_id:'default'});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
});

for(const theme of ['light','dark'])test(`Chinese first run fits a narrow ${theme} window`,async({page})=>{
  // Start in English to share the initial assertion, then change the saved locale.
  await setup(page);
  await page.setViewportSize({width:360,height:560});
  await page.evaluate(theme=>{const w=window as any;w.settings.ui_language='zh';w.settings.theme=theme;w.emit('configuration-changed',structuredClone(w.settings));},theme);
  await expect(page.getByRole('heading',{name:'选择你需要的取词方式'})).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await page.screenshot({path:`test-results/first-use-${theme}-zh.png`,fullPage:true});
});


test('tutorial reports unavailable global shortcuts with a direct settings action',async({page})=>{
  await setup(page);
  await page.evaluate(()=>{(window as any).shortcutConflict=true;});
  await page.getByRole('button',{name:/3\s*Practice/}).click();
  await page.evaluate(()=>window.dispatchEvent(new Event('focus')));
  await expect(page.getByText(/Global shortcuts could not be registered/)).toBeVisible();
  await page.getByRole('button',{name:'Change shortcuts',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='open_settings_at').at(-1).args.route)).toEqual({section:'translation',profile_id:null});
});


test('a skipped guide can be replayed without closing on a progress update',async({page})=>{
  await setup(page);
  await page.getByRole('button',{name:'Set up later',exact:true}).click();
  await page.getByRole('button',{name:'Usage guide',exact:true}).click();
  await page.getByRole('button',{name:'Continue to services',exact:true}).click();
  await expect(page.getByRole('heading',{name:'Choose a service before your first query'})).toBeVisible();
  await page.getByRole('button',{name:'Practice the basics',exact:true}).click();
  await expect(page.getByRole('heading',{name:'Three steps to get started'})).toBeVisible();
});

test('D activates the tutorial sample without sending a real translation',async({page})=>{
 await setup(page);
 await page.getByRole('button',{name:/3\s*Practice/}).click();
 await page.keyboard.press('d');
 await expect(page.locator('.demo-result')).toBeVisible();
 expect(await networkActions(page)).toEqual([]);
});


test('tutorial shortcuts ignore modifiers, repeat, composition, editing and dialogs',async({page})=>{
 await setup(page);
 await page.getByRole('button',{name:/3\s*Practice/}).click();
 await page.evaluate(()=>{
  for(const options of [{repeat:true},{isComposing:true},{altKey:true},{metaKey:true},{ctrlKey:true},{shiftKey:true}])window.dispatchEvent(new KeyboardEvent('keydown',{key:'d',...options}));
  const input=document.createElement('input');document.querySelector('.guide-demo')!.append(input);input.focus();input.dispatchEvent(new KeyboardEvent('keydown',{key:'s',bubbles:true}));input.remove();
  const dialog=document.createElement('dialog');document.body.append(dialog);dialog.showModal();window.dispatchEvent(new KeyboardEvent('keydown',{key:'d'}));dialog.remove();
 });
 await expect(page.locator('.demo-result')).toHaveCount(0);
 await page.getByRole('button',{name:'Translate sample (D / S)',exact:true}).click();
 await expect(page.locator('.demo-result')).toBeVisible();
 expect(await networkActions(page)).toEqual([]);
 await page.setViewportSize({width:360,height:650});
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.screenshot({path:'test-results/simple-guide.png',fullPage:true});
});


test('help toggles the same persistent skip action, preserves the draft and removes redundant empty hints',async({page})=>{
 await setup(page);
 const help=page.getByRole('button',{name:'Usage guide',exact:true});
 await expect(help).toHaveAttribute('aria-expanded','true');
 await help.click();
 await expect(help).toHaveAttribute('aria-expanded','false');
 await expect(page.locator('.getting-started')).toHaveCount(0);
 expect(await page.evaluate(()=>(window as any).progress.setup)).toBe('skipped');
 await expect(page.locator('.start-hint')).toHaveCount(0);
 await expect(page.getByRole('textbox',{name:'Source text',exact:true})).toBeVisible();
 await page.reload();await expect(help).toHaveAttribute('aria-expanded','false');
 const source=page.getByRole('textbox',{name:'Source text',exact:true});await source.fill('An unsent synthetic draft.');
 await help.click();await page.getByRole('button',{name:/3\s*Practice/}).click();
 await help.focus();await page.keyboard.press('Enter');
 await expect(source).toHaveValue('An unsent synthetic draft.');await expect(help).toBeFocused();
 expect(await page.evaluate(()=>(window as any).progress.tutorial)).toBe('new');
 expect(await networkActions(page)).toEqual([]);
});

test('failed or pending skip retains the guide and never silently loses progress',async({page})=>{
 await setup(page);
 const help=page.getByRole('button',{name:'Usage guide',exact:true});
 await page.evaluate(()=>{(window as any).failProgress=true;});await help.click();
 await expect(page.getByRole('alert')).toContainText('Synthetic progress save failed');
 await expect(help).toHaveAttribute('aria-expanded','true');
 expect(await page.evaluate(()=>(window as any).progress.setup)).toBe('new');
 await page.evaluate(()=>{(window as any).failProgress=false;(window as any).deferProgress=true;});
 await page.getByRole('button',{name:'Set up later',exact:true}).click();
 await expect(help).toBeDisabled();await expect(page.getByRole('button',{name:'Set up later',exact:true})).toBeDisabled();
 await page.evaluate(()=>(window as any).releaseProgress());
 await expect(page.locator('.getting-started')).toHaveCount(0);await expect(help).toBeEnabled();
});

test('skip remains prominent without motion and its optional halo stops after two pulses',async({page})=>{
 await setup(page);await page.setViewportSize({width:1100,height:820});
 await page.evaluate(()=>{const w=window as any;w.settings.ui_language='zh';w.settings.theme='dark';w.emit('configuration-changed',structuredClone(w.settings));});
 const skip=page.getByRole('button',{name:'稍后设置',exact:true});
 await expect(skip).toBeVisible();await expect(skip).toHaveCSS('color','rgb(174, 213, 182)');
 expect(await skip.evaluate(el=>getComputedStyle(el,'::after').animationName)).toBe('none');
 const box=await skip.boundingBox();expect(box!.height).toBeGreaterThanOrEqual(36);
 await page.screenshot({path:'.ai-notes/guide-clear-exit-dark.png',fullPage:true,animations:'disabled'});
 await page.emulateMedia({reducedMotion:'no-preference'});
 await expect.poll(()=>skip.evaluate(el=>getComputedStyle(el,'::after').animationIterationCount)).toBe('2');
 await page.emulateMedia({reducedMotion:'reduce'});
 await expect.poll(()=>skip.evaluate(el=>getComputedStyle(el,'::after').animationName)).toBe('none');
});
