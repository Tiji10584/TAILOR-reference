#!/usr/bin/env node
// Keep the private key and this issuing tool on the owner's computer, never in a customer package.
import { generateKeyPairSync, createPrivateKey, sign } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

const [command, ...args] = process.argv.slice(2);
const usage = 'Usage: node scripts/license-admin.mjs init <private.pem> <public-key.txt> | issue <private.pem> <device-code> <license.json>';

try {
  if (command === 'init' && args.length === 2) {
    const [privatePath, publicPath] = args.map(path => resolve(path));
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const publicBytes = Buffer.from(publicKey.export({ format: 'jwk' }).x, 'base64url');
    if (publicBytes.length !== 32) throw new Error('Invalid Ed25519 public key');
    mkdirSync(dirname(privatePath), { recursive: true });
    writeFileSync(privatePath, privateKey.export({ type: 'pkcs8', format: 'pem' }), { flag: 'wx', mode: 0o600 });
    writeFileSync(publicPath, publicBytes.toString('hex') + '\n', { flag: 'wx' });
    console.log(`Created private key at ${privatePath} and public key at ${publicPath}. Do not share the private key.`);
  } else if (command === 'issue' && args.length === 3) {
    const [privatePath, rawCode, outputPath] = args;
    const deviceCode = rawCode.trim().toLowerCase();
    if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(deviceCode)) {
      throw new Error('Invalid device code; copy the code shown by the app.');
    }
    const privateKey = createPrivateKey(readFileSync(resolve(privatePath)));
    if (privateKey.asymmetricKeyType !== 'ed25519') throw new Error('The private key must be Ed25519.');
    const signature = sign(null, Buffer.from(`TAILOR-LICENSE-v1:${deviceCode}`, 'utf8'), privateKey).toString('base64');
    const license = { version: 1, deviceCode, signature };
    mkdirSync(dirname(resolve(outputPath)), { recursive: true });
    writeFileSync(resolve(outputPath), JSON.stringify(license, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
    console.log(`Created license for ${deviceCode} at ${resolve(outputPath)}.`);
  } else {
    throw new Error(usage);
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
