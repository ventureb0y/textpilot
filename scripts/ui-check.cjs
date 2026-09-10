/* Run with Vite running: node scripts/ui-check.cjs.
 * Requires Playwright on NODE_PATH (or installed locally) and Microsoft Edge.
 * Tauri is mocked in an isolated browser context: no user data is read or changed.
 */
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const fixture = require('./fixtures/ui-dashboard.json');
const output = path.resolve('docs/ui-review/after');
fs.mkdirSync(output, { recursive: true });
let browser;
let currentPage;
const passed = [];
const errors = [];
async function open({label = 'main', width = 1100, height = 720, scale = 1, long = false, empty = false} = {}) {
  const context = await browser.newContext({viewport:{width,height},deviceScaleFactor:scale});
  const page = await context.newPage();
  currentPage = page;
  page.on('pageerror', e => errors.push(e.message));
  const data = structuredClone(fixture);
  if (empty) { data.phrases=[]; data.dictionaryWords=[]; }
  if (long) {
    data.categories.push({id:4,parentId:1,name:'Очень длинная вложенная категория',path:'Продажи/Очень длинная вложенная категория',phraseCount:1,wordCount:0});
    data.phrases = Array.from({length:45}, (_,i)=>({...data.phrases[i%4],id:i+1,categoryId:i===0?4:1,title:`Фраза ${i+1} — ${i===0?'Очень длинное название с кириллицей и подробным пояснением':data.phrases[i%4].title}`,isEnabled:i!==1}));
    data.dictionaryWords.push({...data.dictionaryWords[0],id:3,word:'электрофотополупроводниковый',isEnabled:false});
  }
  await page.addInitScript(({data,label})=>{
    const callbacks = {}; let next = 0;
    window.__uiTest = { data, calls: [], listeners:{}, fail:'', hold:'', release:null, closed:false };
    const state = window.__uiTest;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener(){} };
    window.__TAURI_INTERNALS__ = {
      metadata:{currentWindow:{label}},
      transformCallback(fn) { callbacks[++next]=fn; return next; },
      async invoke(cmd,args={}) {
        state.calls.push({cmd,args});
        if(state.hold===cmd) await new Promise(resolve=>{state.release=resolve;});
        if(state.fail===cmd) throw new Error('Тестовая ошибка сохранения');
        if(cmd==='dashboard_data') return structuredClone(data);
        if(cmd==='plugin:app|version') return '0.0.6';
        if(cmd==='plugin:event|listen') { state.listeners[args.event] = callbacks[args.handler]; return next; }
        if(cmd==='plugin:event|unlisten'||cmd==='plugin:updater|check') return null;
        if(cmd==='list_backups'||cmd==='list_profile_snapshots') return [];
        if(cmd==='create_phrase') { data.phrases.push({...args.input,id:99,isEnabled:true}); return null; }
        if(cmd==='update_phrase') { Object.assign(data.phrases.find(p=>p.id===args.input.id),args.input); return null; }
        if(cmd==='set_phrase_enabled') { data.phrases.find(p=>p.id===args.phraseId).isEnabled=args.enabled; return null; }
        if(cmd==='save_dictionary_word') { const word=data.dictionaryWords.find(p=>p.id===args.input.id); if(word)Object.assign(word,args.input); else data.dictionaryWords.push({...args.input,id:99}); return null; }
        if(cmd==='quick_search_items') return Array.from({length:45},(_,i)=>({id:i+1,kind:'phrase',title:`Фраза ${i+1}`,shortcut:`ф${i+1}`,body:'Текст для проверки вставки и прокрутки',category:'Продажи'}));
        if(cmd==='insert_quick_search_item') return null;
        if(cmd==='close_quick_search') {state.closed=true;return null;}
        if(cmd==='select_autocomplete'||cmd==='accept_autocomplete')return null;
        if(cmd==='set_paused')return args.paused;
        if(cmd==='set_quick_search_enabled'||cmd==='set_dictionary_autocomplete_enabled')return args.enabled;
        throw new Error('Unexpected mock command: '+cmd);
      }
    };
  },{data,label});
  await page.goto(process.env.TEXTPILOT_UI_URL || 'http://localhost:1420');
  if(label==='main') await page.getByRole('heading',{name:'Фразы',exact:true}).waitFor();
  else {
    const event=label==='quick-search'?'quick-search-opened':'autocomplete-suggestion';
    await page.waitForFunction(event=>Boolean(window.__uiTest.listeners[event]),event);
    await page.evaluate(({event,label})=>window.__uiTest.listeners[event]({payload:label==='autocomplete'?{prefix:'пред',words:['предложение','предприятие','представление'],selectedIndex:1}:undefined}),{event,label});
    if(label==='quick-search')await page.locator('.results button').first().waitFor();
    else await page.locator('.completion-popup').waitFor();
  }
  return page;
}
async function shot(page,name) { await page.screenshot({path:path.join(output,name+'.png')}); }
async function nav(page,name) {await page.getByRole('navigation').getByTitle(name,{exact:true}).click();}
async function focused(locator) {assert(await locator.evaluate(el=>el===document.activeElement));}
async function test(name,fn) { await fn();passed.push(name);console.log('PASS '+name); }
async function discard(page) {
  await page.getByRole('alertdialog').getByRole('button',{name:'Закрыть без сохранения',exact:true}).click();
  await page.getByRole('alertdialog').waitFor({state:'hidden'});
}
async function run() {
 browser=await chromium.launch({headless:true,channel:process.env.TEXTPILOT_BROWSER || 'msedge'});
 await test('Unchanged editor, initial focus and focus restoration',async()=>{
  const p=await open();const trigger=p.getByRole('button',{name:'Новая фраза',exact:true});await trigger.click();
  await focused(p.getByLabel('Название',{exact:true}));
  await p.keyboard.press('Escape');assert.equal(await p.getByRole('dialog').count(),0);await focused(trigger);
  await shot(p,'phrases');await p.context().close();
 });
 await test('Dirty editor: backdrop, Escape, cancel, focus trap, discard',async()=>{
  const p=await open();await p.getByRole('button',{name:'Новая фраза',exact:true}).click();
  await p.getByLabel('Название',{exact:true}).fill('Несохранённая фраза');
  await p.mouse.click(5,5);await p.getByRole('alertdialog').waitFor();
  await focused(p.getByRole('button',{name:'Продолжить редактирование'}));
  await shot(p,'unsaved-confirmation');
  await p.keyboard.press('Escape');assert.equal(await p.getByRole('dialog').count(),1);assert.equal(await p.getByLabel('Название',{exact:true}).inputValue(),'Несохранённая фраза');
  for(let i=0;i<16;i++){await p.keyboard.press('Tab');assert(await p.evaluate(()=>Boolean(document.activeElement.closest('[role="dialog"]'))));}
  await p.getByRole('button',{name:'Отмена',exact:true}).click();await discard(p);assert.equal(await p.getByRole('dialog').count(),0);
  await p.context().close();
 });
 await test('Edited values reverted to original close without confirmation',async()=>{
  const p=await open();await p.getByRole('button',{name:'Новая фраза',exact:true}).click();const input=p.getByLabel('Название',{exact:true});await input.fill('тест');await input.fill('');await p.keyboard.press('Escape');assert.equal(await p.getByRole('alertdialog').count(),0);assert.equal(await p.getByRole('dialog').count(),0);await p.context().close();
 });
 await test('Saving guard and failure preserve editor values',async()=>{
  const p=await open();await p.getByRole('button',{name:'Новая фраза',exact:true}).click();
  await p.getByLabel('Название',{exact:true}).fill('Тест сохранения');await p.getByLabel('Сокращение',{exact:true}).fill('тест');await p.getByLabel('Текст фразы',{exact:true}).fill('Текст остаётся после ошибки');
  await p.evaluate(()=>{window.__uiTest.hold='create_phrase';window.__uiTest.fail='create_phrase';});
  await p.getByRole('button',{name:'Создать фразу',exact:true}).click();await p.waitForFunction(()=>Boolean(window.__uiTest.release));
  await p.keyboard.press('Escape');assert.equal(await p.getByRole('dialog').count(),1);assert(await p.getByLabel('Название',{exact:true}).isDisabled());assert(await p.getByTitle('Закрыть',{exact:true}).isDisabled());
  await p.evaluate(()=>window.__uiTest.release());await p.getByText('Не удалось сохранить изменения.',{exact:true}).waitFor();
  assert.equal(await p.getByLabel('Текст фразы',{exact:true}).inputValue(),'Текст остаётся после ошибки');
  assert.equal(await p.evaluate(()=>window.__uiTest.calls.filter(c=>c.cmd==='create_phrase').length),1);
  await p.evaluate(()=>{window.__uiTest.hold='';window.__uiTest.fail='';});await p.getByRole('button',{name:'Создать фразу',exact:true}).click();await p.getByRole('dialog').waitFor({state:'hidden'});await p.getByText('Тест сохранения',{exact:true}).waitFor();
  await p.context().close();
 });
 await test('Phrase menu: isolated action, keyboard navigation, outside click',async()=>{
  const p=await open();const trigger=p.getByRole('button',{name:'Действия с фразой «Коммерческое предложение»',exact:true});
  await trigger.focus();await p.keyboard.press('ArrowDown');await focused(p.getByRole('menuitem',{name:'Редактировать'}));assert.equal(await p.locator('.detail-panel').count(),0);
  await p.keyboard.press('End');await focused(p.getByRole('menuitem',{name:'Удалить'}));await p.keyboard.press('Home');await p.keyboard.press('Escape');await focused(trigger);
  await trigger.click();await shot(p,'phrase-menu');await p.getByRole('heading',{name:'Фразы',exact:true}).click();assert.equal(await p.getByRole('menu').count(),0);
  await trigger.click();await p.getByRole('menuitem',{name:'Редактировать'}).click();await focused(p.getByLabel('Название',{exact:true}));await p.keyboard.press('Escape');await focused(trigger);
  await trigger.click();await p.getByRole('menuitem',{name:'Редактировать'}).click();await p.getByLabel('Текст фразы',{exact:true}).fill('Обновлённый текст');await p.getByRole('button',{name:'Сохранить изменения',exact:true}).click();await p.getByRole('dialog').waitFor({state:'hidden'});await focused(trigger);await p.context().close();
 });
 await test('Dictionary switches, word editor and keyboard menu',async()=>{
  const p=await open();await nav(p,'Словарь');await shot(p,'dictionary');
  const card=p.locator('.dictionary-card').first();const toggle=card.getByRole('button',{name:'Автодополнение',exact:true});assert.equal(await toggle.getAttribute('aria-pressed'),'true');await toggle.click();await p.waitForFunction(()=>document.querySelector('.dictionary-meta button:nth-child(2)').getAttribute('aria-pressed')==='false');
  await card.getByRole('button',{name:/Действия со словом/}).click();await p.getByRole('menuitem',{name:'Редактировать'}).click();await p.getByLabel('Правильное слово').fill('согласование');await p.keyboard.press('Escape');await discard(p);
  await p.getByRole('button',{name:'Добавить слово',exact:true}).click();await p.keyboard.press('Escape');assert.equal(await p.getByRole('dialog').count(),0);await p.context().close();
 });
 await test('Category and profile forms protect unsaved values',async()=>{
  const p=await open();await p.getByTitle('Добавить категорию',{exact:true}).click();await p.getByLabel('Название',{exact:true}).fill('Новая категория');await p.getByTitle('Закрыть',{exact:true}).click();await discard(p);
  await nav(p,'Профили');await shot(p,'profiles');await p.getByRole('button',{name:/Новый профиль|Создать профиль/}).first().click();await p.getByLabel('Название профиля').fill('Новый профиль');await p.keyboard.press('Escape');await discard(p);await p.context().close();
 });
 await test('Narrow view restores filter, scroll and switches back on category change',async()=>{
  const p=await open({long:true});await p.locator('.phrase-main').evaluate(el=>el.scrollTop=500);
  const row=p.locator('.phrase-open').nth(6);await row.click();assert(!await p.locator('.phrase-main').isVisible());assert(await p.locator('.detail-panel').isVisible());await shot(p,'detail-1100');await p.getByRole('button',{name:'← Назад'}).click();assert(await p.locator('.phrase-main').isVisible());assert((await p.locator('.phrase-main').evaluate(el=>el.scrollTop))>0);
  await row.click();await p.getByRole('button',{name:'Без категории',exact:false}).first().click();assert.equal(await p.locator('.detail-panel').count(),0);await p.getByText('В этой категории пока нет фраз',{exact:true}).waitFor();await p.context().close();
 });
 await test('Selected category metadata retains child category and disabled state',async()=>{
  const p=await open({long:true});await p.getByTitle('Продажи',{exact:true}).click();assert.equal(await p.locator('.phrase-meta').nth(0).innerText(),'Продажи/Очень длинная вложенная категория');assert.equal(await p.locator('.phrase-meta').nth(1).innerText(),'Выключена');await p.context().close();
 });
 await test('Data settings grouped, histories collapsed and scope explicit',async()=>{
  const p=await open();await nav(p,'Настройки');await shot(p,'settings');await p.locator('.data-settings').scrollIntoViewIfNeeded();await shot(p,'data-settings');assert.equal(await p.locator('.backup-history[open]').count(),0);assert.equal(await p.locator('.backup-history').count(),2);await p.locator('.backup-history summary').first().click();await p.getByText('У этого профиля пока нет снимков.',{exact:true}).waitFor();await p.context().close();
 });
 await test('Quick search: visible keyboard selection, empty Enter and insertion error',async()=>{
  const p=await open({label:'quick-search',width:520,height:380});
  for(let i=0;i<25;i++)await p.keyboard.press('ArrowDown');
  await p.waitForFunction(()=>{const r=document.querySelector('.results').getBoundingClientRect(),s=document.querySelector('.results .selected').getBoundingClientRect();return s.top>=r.top&&s.bottom<=r.bottom;});await shot(p,'quick-search-520');
  await p.evaluate(()=>window.__uiTest.fail='insert_quick_search_item');await p.keyboard.press('Enter');await p.getByRole('alert').waitFor();assert.equal(await p.locator('.results button').count(),45);await shot(p,'quick-search-error');
  await p.getByRole('textbox').fill('нет совпадений');const before=await p.evaluate(()=>window.__uiTest.calls.filter(c=>c.cmd==='insert_quick_search_item').length);await p.keyboard.press('Enter');assert.equal(await p.evaluate(()=>window.__uiTest.calls.filter(c=>c.cmd==='insert_quick_search_item').length),before);
  await p.getByRole('textbox').fill('Фраза 3');assert.equal(await p.locator('.results .selected strong').innerText(),'Фраза 3');await p.keyboard.press('Escape');assert(await p.evaluate(()=>window.__uiTest.closed));await p.context().close();
 });
 await test('Layout matrix and browser pixel-density snapshots',async()=>{
  for(const [width,height] of [[820,560],[1100,720],[1440,900]]) for(const scale of [1,1.25,1.5]) {
    const p=await open({width,height,scale,long:true});
    assert(await p.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth),'horizontal overflow');
    await p.locator('.phrase-open').first().click();assert.equal(await p.locator('.phrase-main').isVisible(),width>=1200);
    const bounds=await p.locator('.detail-panel').boundingBox();assert(bounds.x+bounds.width<=width+1);
    await shot(p,`detail-${width}-${scale}`);
    await p.getByRole('button',{name:'Редактировать',exact:true}).click();
    const modal=p.getByRole('dialog');assert((await modal.boundingBox()).height<=height);
    await p.getByRole('button',{name:'Сохранить изменения',exact:true}).scrollIntoViewIfNeeded();await shot(p,`editor-${width}-${scale}`);
    await p.keyboard.press('Escape');await nav(p,'Словарь');
    assert(await p.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth),'dictionary horizontal overflow');await shot(p,`dictionary-${width}-${scale}`);
    await p.context().close();
  }
  for(const scale of [1,1.25,1.5]) {
    const p=await open({label:'quick-search',width:680,height:500,scale});await shot(p,`quick-search-680-${scale}`);await p.context().close();
    const o=await open({label:'autocomplete',width:360,height:174,scale});
    assert(await o.evaluate(()=>{const row=[...document.querySelectorAll('.completion-list button')].at(-1).getBoundingClientRect();return row.bottom<=document.querySelector('footer').getBoundingClientRect().top;}),'overlay rows clipped');await shot(o,`autocomplete-${scale}`);await o.context().close();
  }
 });
 assert.deepEqual(errors,[], 'Uncaught browser errors');
 fs.writeFileSync(path.join(output,'checks.json'),JSON.stringify({passed,browserErrors:errors,note:'Browser tests use mocked Tauri commands. DeviceScaleFactor is not a real Windows DPI test.'},null,2)+'\n');
 await browser.close();console.log(`${passed.length} checks passed`);
}
run().catch(async e=>{console.error(e);if(currentPage&&!currentPage.isClosed())await shot(currentPage,'failure').catch(()=>{});await browser?.close();process.exitCode=1;});

