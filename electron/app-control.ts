import { BrowserWindow, session, type IpcMainInvokeEvent } from 'electron';
import path from 'node:path';
import { appRequest, appEventAllowed } from './app-control-rules.ts';

// An isolated world keeps refs out of application code. Never return input values or accept agent JavaScript.
const SNAPSHOT = `(() => {
 const s = globalThis.__appRefs ||= {next:0, ids:new WeakMap(), nodes:new Map()};
 s.nodes.clear();
 const rows = [];
 for (const el of document.querySelectorAll('button,a,input,textarea,select,[role="button"],[role="option"],[role="menuitem"],[role="tab"],[role="checkbox"],[role="radio"],[role="switch"],[contenteditable="true"]')) {
  const b=el.getBoundingClientRect(); if (!b.width || !b.height || b.bottom<0 || b.top>innerHeight) continue;
  if (el.matches('input[type="password"],input[autocomplete*="password"],input[autocomplete="one-time-code"]')) continue;
  let id=s.ids.get(el); if (!id) {id=++s.next;s.ids.set(el,id);}
  const label=(el.getAttribute('aria-label')||el.innerText||el.getAttribute('placeholder')||el.getAttribute('name')||el.tagName).slice(0,160);
  s.nodes.set(id,{el,label,role:el.getAttribute('role')}); rows.push(id+' '+el.tagName+' '+label); if(rows.length>=500) break;
 }
 return {title:document.title,text:document.body.innerText.slice(0,18000)+'\\n'+rows.join('\\n')};
})()`;
class InvalidRef extends Error {}
type Entry = { window: BrowserWindow; conversation: string; authorized: Set<string>; idle?: NodeJS.Timeout; busy: boolean };
// A single reusable memory profile bounds Electron's retained Session objects.
export function appControlSession() {return session.fromPartition('app-control', {cache:false});}
let profileOpening=false;
export class AppControl {
  private entries = new Map<string, Entry>();
  private opening?: {conversation: string; cancelled: boolean};
  private testing: boolean;
  private origin: string;
  constructor(testing: boolean, origin: string) {this.testing=testing;this.origin=origin;}
  owner(event: IpcMainInvokeEvent): string | undefined {
    for (const e of this.entries.values()) {
      if(e.window.isDestroyed()) continue;
      const c=e.window.webContents, f=event.senderFrame;
      if (!e.window.isDestroyed() && !c.isDestroyed() && event.sender===c && f && f.processId===c.mainFrame.processId && f.routingId===c.mainFrame.routingId && f.url.startsWith(this.origin+'/')) return e.conversation;
    }
  }
  async dispatch(event: IpcMainInvokeEvent, command: string, args: Record<string, unknown>, call: (command: string, args: Record<string, unknown>) => Promise<unknown>): Promise<unknown> {
    const conversation=this.owner(event), entry=conversation ? this.entries.get(conversation) : undefined;
    if (!entry) throw new Error('app session closed');
    const result=await call('__app_agent', {conversation, command, args});
    // Only the checked host response grants a subscription, tied to this renderer's lifetime.
    if (command==='open' && result && typeof result==='object' && !Array.isArray(result)) {
      const id=(result as Record<string, unknown>).id;
      if (typeof id==='string' && id.length>0 && id.length<=200 && this.entries.get(conversation!)===entry && this.owner(event)===conversation) entry.authorized.add(id);
    }
    return result;
  }
  tell(channel: string, payload: unknown): void {
    if(channel!=='faber:event') return;
    if(payload && typeof payload==='object' && !Array.isArray(payload)) {
      const event=payload as Record<string, unknown>, message=event.message;
      if(event.type==='message' && typeof event.conversationId==='string' && message && typeof message==='object' && !Array.isArray(message) && ['done','stopped','error'].includes(String((message as Record<string, unknown>).status))) {
        if(this.opening?.conversation===event.conversationId)this.opening.cancelled=true;
        const e=this.entries.get(event.conversationId);
        if(e && this.entries.get(event.conversationId)===e) {this.entries.delete(event.conversationId);clearTimeout(e.idle);if(!e.window.isDestroyed())e.window.destroy();}
      }
    }
    for(const e of this.entries.values()) if(!e.window.isDestroyed() && !e.window.webContents.isDestroyed() && appEventAllowed(payload,e.authorized)) e.window.webContents.send(channel,payload);
  }
  closeAll(): void { if(this.opening)this.opening.cancelled=true;for(const e of this.entries.values()) {clearTimeout(e.idle);e.window.destroy();} this.entries.clear(); }
  async act(conversation: string, input: unknown): Promise<unknown> {
    const p=appRequest(input,this.testing);
    if (!conversation || conversation.length>200) throw new Error('missing app session');
    let e=this.entries.get(conversation);
    if(p.action==='close') { if(this.opening?.conversation===conversation)this.opening.cancelled=true;if(e) {clearTimeout(e.idle);e.window.destroy();this.entries.delete(conversation);} return {title:p.app,text:'App session closed.'}; }
    if(e?.busy) throw new Error('app session busy');
    if(!e) {
      const profile=appControlSession();
      if(this.entries.size>=1 || profileOpening) throw new Error('close an app session first');
      if(BrowserWindow.getAllWindows().some(w=>!w.isDestroyed() && w.webContents.session===profile))throw new Error('close an app session first');
      profileOpening=true;
      const opening={conversation,cancelled:false};this.opening=opening;
      try {
        await profile.clearStorageData();
        await profile.clearCache();
        await profile.closeAllConnections();
        profile.setPermissionRequestHandler((_wc,_permission,callback)=>callback(false));
        profile.setPermissionCheckHandler(()=>false);
        profile.setDevicePermissionHandler(()=>false);
        profile.removeAllListeners('will-download');
        profile.on('will-download',event=>event.preventDefault());
      } finally {profileOpening=false;this.opening=undefined;}
      if(opening.cancelled)throw new Error('app session closed');
      const window=new BrowserWindow({show:false,width:1280,height:800,webPreferences:{session:profile,preload:path.join(import.meta.dirname,'preload.cjs'),sandbox:true,contextIsolation:true,nodeIntegration:false,webviewTag:false,backgroundThrottling:false,offscreen:true}});
      e={window,conversation,authorized:new Set([conversation]),busy:true}; this.entries.set(conversation,e);
      const c=window.webContents;
      c.setFrameRate(10);
      c.setWindowOpenHandler(()=>({action:'deny'}));
      c.on('will-navigate',(ev,url)=>{if(!url.startsWith(this.origin+'/')) ev.preventDefault();});
      c.on('will-redirect',(ev,url)=>{if(!url.startsWith(this.origin+'/')) ev.preventDefault();});
      c.on('will-attach-webview',ev=>ev.preventDefault());
      try {await window.loadURL(this.origin+'/index.html');} catch(err) {window.destroy();if(this.entries.get(conversation)===e)this.entries.delete(conversation);throw err;}
    }
    e.busy=true;clearTimeout(e.idle);
    try {
      if(p.size) {const [width,height]=p.size.split('x').map(Number);e.window.setContentSize(width!,height!);}
      const c=e.window.webContents;
      if(!c.debugger.isAttached()) c.debugger.attach('1.3');
      const call=(method:string,params:Record<string,unknown>={})=>{if(e!.window.isDestroyed() || this.entries.get(conversation)!==e) throw new Error('app session closed');return c.debugger.sendCommand(method,params);};
      const tree=await call('Page.getFrameTree');
      const world=await call('Page.createIsolatedWorld',{frameId:tree.frameTree.frame.id,worldName:'faber-app-control'});
      const evaluate=async(expression:string, validation=false)=>{const r=await call('Runtime.evaluate',{expression,contextId:world.executionContextId,returnByValue:true});if(r.exceptionDetails) {if(validation) throw new InvalidRef('stale ref; read again');throw new Error('stale ref; read again');}return r.result.value;};
      if(p.action==='click'||p.action==='type') {
        const ref=JSON.stringify(p.ref);
        const point=await evaluate(`(() => {const n=globalThis.__appRefs?.nodes.get(${ref}); if(!n || !n.el.isConnected || n.el.matches('input[type="password"],input[autocomplete*="password"],input[autocomplete="one-time-code"]')) throw Error(); const el=n.el; if(el.getAttribute('role')!==n.role) throw Error(); if(el.matches(':disabled,[aria-disabled="true"]') || (${p.action==='type'} && (!el.matches('input:not([type]),input[type="text"],input[type="search"],input[type="email"],input[type="url"],input[type="tel"],input[type="number"],textarea,[contenteditable="true"]') || el.readOnly))) throw Error(); const b=el.getBoundingClientRect(),x=b.x+b.width/2,y=b.y+b.height/2; if(x<0||y<0||x>=innerWidth||y>=innerHeight||!el.contains(document.elementFromPoint(x,y))) throw Error(); const label=(el.getAttribute('aria-label')||el.innerText||el.getAttribute('placeholder')||el.getAttribute('name')||el.tagName).slice(0,160);if(label!==n.label)throw Error();return {x,y};})()`,true);
        await call('Input.dispatchMouseEvent',{type:'mousePressed',...point,button:'left',clickCount:1});
        await call('Input.dispatchMouseEvent',{type:'mouseReleased',...point,button:'left',clickCount:1});
        if(p.action==='type') {
          await evaluate(`(() => { const el=document.activeElement; if(el!==globalThis.__appRefs?.nodes.get(${ref})?.el || !el || el.matches(':disabled,[aria-disabled="true"]') || el.readOnly || !el.matches('input:not([type="password"]),textarea,[contenteditable="true"]') || el.matches('[autocomplete*="password"],[autocomplete="one-time-code"]')) throw Error(); })()`,true);
          await call('Input.insertText',{text:p.text});
        }
      } else if(p.action==='scroll') await call('Input.dispatchMouseEvent',{type:'mouseWheel',x:640,y:400,deltaX:0,deltaY:p.up?-600:600});
      const [expectedWidth,expectedHeight]=e.window.getContentSize();
      const captured=await call('Page.captureScreenshot',{format:'jpeg',quality:65,captureBeyondViewport:false,fromSurface:true,clip:{x:0,y:0,width:expectedWidth,height:expectedHeight,scale:1}});
      const picture=Buffer.from(captured.data,'base64');
      const snapshot=await evaluate(SNAPSHOT);
      return {...snapshot,picture:picture.toString('base64')};
    } catch(error) {
      if(!(error instanceof InvalidRef)) {
        if(this.entries.get(conversation)===e)this.entries.delete(conversation);
        if(!e.window.isDestroyed())e.window.destroy();
      }
      throw error;
    } finally {
      e.busy=false;
      if (!e.window.isDestroyed() && this.entries.get(conversation)===e) {
        e.idle=setTimeout(()=>{e!.window.destroy();if(this.entries.get(conversation)===e)this.entries.delete(conversation);},60000);e.idle.unref();
      }
    }
  }
}
