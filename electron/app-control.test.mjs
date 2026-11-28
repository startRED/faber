import assert from 'node:assert/strict';
import { test } from 'node:test';
import { appRequest, appEventAllowed } from './app-control-rules.ts';

test('agent events deny private foreign and unknown payloads and retain own stream and shared refresh', () => {
  const authorized=new Set(['owner']);
  for(const type of ['message','queue','commands','asides','context','contextParts','plan','halted']) {
    assert.equal(appEventAllowed({type,conversationId:'owner'},authorized),true);
    assert.equal(appEventAllowed({type,conversationId:'other',message:'private'},authorized),false);
  }
  for(const type of ['running','background','asks','pane','unknown']) {
    assert.equal(appEventAllowed({type,conversationId:'owner',asks:['private']},authorized),false);
    assert.equal(appEventAllowed({type,asks:['private']},authorized),false);
  }
  for(const type of ['list','catalog','quota','usage','settings','lagged']) assert.equal(appEventAllowed({type},authorized),true);
  for(const payload of [null, [], {}, {type:'settings',conversationId:'other'}]) assert.equal(appEventAllowed(payload,authorized),false);
});
test('app attachment requires an explicit matching backend and rejects script and credential input', () => {
  assert.deepEqual(appRequest({app:'faber-live', action:'read'}, false), {app:'faber-live', action:'read'});
  for (const input of [{app:'faber-test'}, {app:'faber-live',action:'evaluate'}, {app:'faber-live',action:'type',ref:'1',text:'secret',password:true}]) {
    assert.throws(() => appRequest(input, false));
  }
  assert.throws(() => appRequest({app:'faber-live'}, true));
});

test('app inputs are bounded and ordinary browsing options cannot grant script execution', () => {
  for (const p of [null, {}, {app:'faber-live',action:'click',ref:'button'}, {app:'faber-live',action:'type',ref:1,text:'x'.repeat(2001)}, {app:'faber-live',size:'1x1'}, {app:'faber-live',url:'https://example.com'}, {app:'faber-live',conversation:'another'}]) assert.throws(()=>appRequest(p,false));
  assert.equal(appRequest({app:'faber-test',size:'390x844'},true).size,'390x844');
});
