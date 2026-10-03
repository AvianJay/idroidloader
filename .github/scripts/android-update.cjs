const fs = require('node:fs');
const crypto = require('node:crypto');

function createManifest(badging, apk, channel) {
  if (!['release', 'nightly'].includes(channel)) throw new Error('Invalid update channel');
  const match = badging.match(/^package: name='([^']+)' versionCode='(\d+)' versionName='([^']+)'/m);
  if (!match || match[1] !== 'app.idroidloader.mobile') throw new Error('Unexpected APK package');
  const versionCode = Number(match[2]);
  if (!Number.isSafeInteger(versionCode) || versionCode <= 100_000_000 || versionCode > 2_100_000_000) {
    throw new Error('APK must use the shared CI version code sequence');
  }
  if (channel === 'nightly' ? !/-nightly\.\d+$/.test(match[3]) : match[3].includes('-')) {
    throw new Error('APK version does not match its update channel');
  }
  if (!/^native-code: 'arm64-v8a'\s*$/m.test(badging)) throw new Error('Expected ARM64 APK');
  return {
    schemaVersion: 1,
    channel,
    packageName: match[1],
    abi: 'arm64-v8a',
    version: match[3],
    versionCode,
    sha256: crypto.createHash('sha256').update(apk).digest('hex'),
    size: apk.length,
  };
}

module.exports = { createManifest };

if (require.main === module) {
  const manifest = createManifest(fs.readFileSync('artifacts/apk-badging.txt', 'utf8'),
    fs.readFileSync('artifacts/iDroidLoader-arm64.apk'), process.env.IDROID_UPDATE_CHANNEL);
  fs.writeFileSync('artifacts/android-update.json', `${JSON.stringify(manifest, null, 2)}\n`);
}
