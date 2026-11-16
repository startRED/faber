import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtempSync,mkdirSync,symlinkSync,rmSync} from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {isolatedTestData} from './app-control-data.ts';
test('test target requires explicit separate data, resolving Windows case and junction aliases without opening default data',()=>{
 const root=mkdtempSync(path.join(os.tmpdir(),'faber-data-guard-'));
 try {
  const normal=path.join(root,'normal'), separate=path.join(root,'test'), alias=path.join(root,'alias');
  mkdirSync(normal);mkdirSync(separate);symlinkSync(normal,alias,'junction');
  assert.equal(isolatedTestData(undefined,normal),false);
  assert.equal(isolatedTestData('',normal),false);
  assert.equal(isolatedTestData(normal,normal),false);
  assert.equal(isolatedTestData(normal.toUpperCase(),normal),false);
  assert.equal(isolatedTestData(alias,normal),false);
  assert.equal(isolatedTestData(path.join(alias,'new'),normal),false);
  assert.equal(isolatedTestData(separate,normal),true);
 } finally {rmSync(root,{recursive:true,force:true});}
});
