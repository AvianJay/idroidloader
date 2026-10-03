const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const { createManifest } = require('../.github/scripts/android-update.cjs');

const apk = Buffer.from('public APK fixture');
const badging = (version, code = 100000123) =>
  `package: name='app.idroidloader.mobile' versionCode='${code}' versionName='${version}' platformBuildVersionName='16'\nnative-code: 'arm64-v8a'\n`;

test('metadata uses the APK version code and actual download hash/size for both channels', () => {
  for (const [channel, version] of [['nightly', '2.3.4-nightly.123'], ['release', '2.3.4']]) {
    const manifest = createManifest(badging(version), apk, channel);
    assert.equal(manifest.version, version);
    assert.equal(manifest.channel, channel);
    assert.equal(manifest.versionCode, 100000123);
    assert.equal(manifest.size, apk.length);
    assert.equal(manifest.sha256, crypto.createHash('sha256').update(apk).digest('hex'));
  }
});

test('rejects wrong packages, ABIs, mismatched channels and non-CI version codes', () => {
  assert.throws(() => createManifest(badging('2.3.4').replace('app.idroidloader.mobile', 'app.unrelated'), apk, 'release'));
  assert.throws(() => createManifest(badging('2.3.4').replace('arm64-v8a', 'x86_64'), apk, 'release'));
  assert.throws(() => createManifest(badging('2.3.4-nightly.123'), apk, 'release'));
  assert.throws(() => createManifest(badging('2.3.4'), apk, 'nightly'));
  assert.throws(() => createManifest(badging('2.3.4', 2003004), apk, 'release'));
});
