import assert from 'node:assert/strict';
import { test } from 'node:test';
import { generateKeyPairSync, verify } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const script = resolve('scripts/license-admin.mjs');
const deviceCode = '42e3b5f7-804e-4aa0-bdcc-97b66dd2f38f';

test('owner issues a dated or permanent signed license and invalid dates fail', () => {
  const folder = mkdtempSync(join(tmpdir(), 'tailor-licenses-'));
  try {
    const {privateKey,publicKey} = generateKeyPairSync('ed25519');
    const keyPath = join(folder, 'owner.pem');
    writeFileSync(keyPath, privateKey.export({type:'pkcs8',format:'pem'}));
    const run = (file, ...choice) => spawnSync(process.execPath,[script,'issue',keyPath,deviceCode,join(folder,file),...choice],{encoding:'utf8'});
    const date = '2090-10-04T20:00+03:00';
    assert.equal(run('dated.json','--expires',date).status,0);
    const timed = JSON.parse(readFileSync(join(folder,'dated.json'),'utf8'));
    assert.equal(timed.expiresAt,Date.parse(date));
    assert.ok(verify(null,Buffer.from(`TAILOR-LICENSE-v2:${deviceCode}:${timed.expiresAt}`),publicKey,Buffer.from(timed.signature,'base64')));
    assert.equal(run('permanent.json','--permanent').status,0);
    const permanent = JSON.parse(readFileSync(join(folder,'permanent.json'),'utf8'));
    assert.equal(permanent.expiresAt,null);
    assert.ok(verify(null,Buffer.from(`TAILOR-LICENSE-v2:${deviceCode}:permanent`),publicKey,Buffer.from(permanent.signature,'base64')));
    assert.notEqual(run('bad.json','--expires','2090-02-31T20:00+03:00').status,0);
    assert.equal(existsSync(join(folder,'bad.json')),false);
    assert.notEqual(run('no-choice.json').status,0);
  } finally {
    rmSync(folder,{recursive:true,force:true});
  }
});
