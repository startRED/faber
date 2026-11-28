import assert from 'node:assert/strict';
import { app, BrowserWindow, ipcMain, protocol, nativeImage } from 'electron';
import { mkdtemp, readFile, mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { AppControl, appControlSession } from '../out/electron/app-control.js';
import { Sidecar } from '../out/electron/sidecar.js';
const root=path.resolve(import.meta.dirname,'..');
const data=await mkdtemp(path.join(os.tmpdir(),'faber-app-proof-'));
process.env.FABER_HIDDEN='1';process.env.FABER_DATA_DIR=data;
app.setPath('userData',path.join(data,'webview'));
app.disableHardwareAcceleration();
app.on('window-all-closed',()=>{});
protocol.registerSchemesAsPrivileged([{scheme:'faber',privileges:{standard:true,secure:true,supportFetchAPI:true}}]);
void app.whenReady().then(async()=>{
 const baseline=app.getAppMetrics().reduce((sum,m)=>sum+m.memory.workingSetSize,0);
 const control=new AppControl(true,'faber://app');
 const timeout=setTimeout(()=>{console.error('FAIL actual app proof timeout');app.exit(1);},60000);
 let host;
 const refusals=[];
 try {
  appControlSession().protocol.handle('faber',async request=>{
   const p=path.resolve(root,'dist','.'+decodeURIComponent(new URL(request.url).pathname));
   assert.ok(p.startsWith(path.join(root,'dist')+path.sep));
   const type={'.js':'text/javascript','.css':'text/css','.html':'text/html','.woff2':'font/woff2','.svg':'image/svg+xml'}[path.extname(p)] || 'application/octet-stream';
   return new Response(await readFile(p),{headers:{'Content-Type':type}});
  });
  const boot=Promise.withResolvers();
  host=Sidecar.start({event:event=>control.tell('faber:event',event),order(){},ended:code=>{if(code)console.error('host ended',code);}});
  ipcMain.handle('faber:invoke',async(event,command,args)=>{
   const conversation=control.owner(event);assert.equal(conversation,'actual-proof');
   if(command==='screen_ready'){boot.resolve();return {ok:null};}
   if(command==='set_clock')return {ok:null};
   try {const ok=await control.dispatch(event,command,args ?? {},(name,input)=>host.call(name,input));return {ok};} catch(err){refusals.push({command,err});console.error('refused',command,err);return {err};}
  });
  await control.act('actual-proof',{app:'faber-test'});
  await boot.promise;
  let view=await control.act('actual-proof',{app:'faber-test'});
  for(const label of ['Nenhuma por enquanto','Continuar']) {
   const row=view.text.split('\n').find(line=>/^\d+ BUTTON /.test(line) && line.includes(label));
   assert.ok(row,`real UI button ${label}`);
   view=await control.act('actual-proof',{app:'faber-test',action:'click',ref:Number(row.split(' ')[0])});
  }
  assert.ok(refusals.some(r=>r.command==='save_settings' && r.err?.key==='error.reach.ask'));
  assert.match(view.text,/Conecte as suas assinaturas/);
  const desktop=await control.act('actual-proof',{app:'faber-test',action:'read',size:'1280x800'});
  const phone=await control.act('actual-proof',{app:'faber-test',action:'read',size:'720x900'});
  console.log('actual visible text',desktop.text.slice(0,300));
  assert.ok(phone.picture.length>1000);
  assert.deepEqual(nativeImage.createFromBuffer(Buffer.from(phone.picture,'base64')).getSize(),{width:720,height:900});
  await mkdir(path.join(root,'.faber','verify'),{recursive:true});
  for(const [name,value] of [['desktop',desktop],['phone',phone]]) await writeFile(path.join(root,'.faber','verify','app-control-'+name+'.jpg'),Buffer.from(value.picture,'base64'));
  const active=app.getAppMetrics().reduce((sum,m)=>sum+m.memory.workingSetSize,0);
  console.log('Chromium working set KiB',JSON.stringify({baseline,active,increment:active-baseline}));
  assert.ok(!BrowserWindow.getAllWindows().some(w=>w.isVisible()));
  console.log('PASS actual Faber UI + real temporary host, desktop and phone screenshots, hidden throughout');
 } catch(error) {console.error('ACTUAL PROOF FAILED',error);process.exitCode=1;} finally {clearTimeout(timeout);control.closeAll();if(host)await host.stop(60000);app.exit(process.exitCode || 0);}
}).catch(error=>{console.error(error);app.exit(1);});
