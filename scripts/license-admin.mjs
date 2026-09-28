#!/usr/bin/env node
// Keep the private key and this issuing tool on the owner's computer, never in a customer package.
import { generateKeyPairSync, createPrivateKey, sign } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

const [command, ...args] = process.argv.slice(2);
const usage = 'Usage: node scripts/license-admin.mjs init <private.pem> <public-key.txt> | issue <private.pem> <device-code> <license.json> --expires YYYY-MM-DDTHH:mm+03:00 | issue <private.pem> <device-code> <license.json> --permanent';

function expiryFromChoice(choice, value) {
  if (choice === '--permanent' && value === undefined) return null;
  if (choice !== '--expires' || typeof value !== 'string') throw new Error(usage);
  const match = value.match(/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(Z|([+-])(\d{2}):(\d{2}))$/);
  if (!match) throw new Error('Enter date and time with a timezone, for example 2026-10-04T20:00+03:00.');
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const hour = Number(match[4]);
  const minute = Number(match[5]);
  const offsetHour = Number(match[8] ?? 0);
  const offsetMinute = Number(match[9] ?? 0);
  const daysInMonth = new Date(Date.UTC(year, month, 0)).getUTCDate();
  if (year<2020 || year>2100 || month<1 || month>12 || day<1 || day>daysInMonth || hour>23 || minute>59 || offsetHour>14 || offsetMinute>59 || offsetHour===14 && offsetMinute>0) {
    throw new Error('Invalid calendar date or time.');
  }
  const expiresAt = Date.parse(value);
  if (!Number.isSafeInteger(expiresAt) || expiresAt <= Date.now()) throw new Error('Expiration must be a future date and time.');
  return expiresAt;
}

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
  } else if (command === 'issue' && (args.length === 4 || args.length === 5)) {
    const [privatePath, rawCode, outputPath, choice, value] = args;
    const expiresAt = expiryFromChoice(choice, value);
    const deviceCode = rawCode.trim().toLowerCase();
    if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(deviceCode)) {
      throw new Error('Invalid device code; copy the code shown by the app.');
    }
    const privateKey = createPrivateKey(readFileSync(resolve(privatePath)));
    if (privateKey.asymmetricKeyType !== 'ed25519') throw new Error('The private key must be Ed25519.');
    const signature = sign(null, Buffer.from(`TAILOR-LICENSE-v2:${deviceCode}:${expiresAt ?? 'permanent'}`, 'utf8'), privateKey).toString('base64');
    const license = { version: 2, deviceCode, expiresAt, signature };
    mkdirSync(dirname(resolve(outputPath)), { recursive: true });
    writeFileSync(resolve(outputPath), JSON.stringify(license, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
    console.log(`Created ${expiresAt === null ? 'permanent' : `timed until ${new Date(expiresAt).toISOString()}`} license for ${deviceCode} at ${resolve(outputPath)}.`);
  } else {
    throw new Error(usage);
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
