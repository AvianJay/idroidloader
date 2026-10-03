const fs = require('node:fs');
const path = require('node:path');

module.exports = async function publish({ github, context, core, readFile = fs.readFileSync, channel = process.env.IDROID_UPDATE_CHANNEL || 'nightly' }) {
  const repo = context.repo;
  if (!['release', 'nightly'].includes(channel)) throw new Error('Invalid update channel');
  const nightly = channel === 'nightly';
  const tag = nightly ? 'nightly' : context.ref.replace(/^refs\/tags\//, '');
  if (!nightly && !/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error('Stable releases require a vX.Y.Z tag');
  const files = ['iDroidLoader-arm64.apk', 'iDroidLoader-arm64.apk.sha256', 'android-update.json'];
  if (nightly) {
    const main = await github.rest.git.getRef({ ...repo, ref: 'heads/main' });
    if (main.data.object.sha !== context.sha) {
      core.notice('A newer main commit exists; its build will update nightly.');
      return;
    }
  }

  const manifest = JSON.parse(readFile(path.join('artifacts', 'android-update.json')).toString());
  if (manifest.channel !== channel || (!nightly && manifest.version !== tag.slice(1))) {
    throw new Error('APK metadata does not match the release channel/tag');
  }
  const build = process.env.IDROID_ANDROID_BUILD ?? String(context.runNumber);
  const name = nightly ? 'iDroidLoader Nightly' : `iDroidLoader ${tag}`;
  const body = [
    nightly ? 'Rolling Android nightly: this release is updated in place after successful builds.' : `Android release ${tag}.`,
    '',
    `Commit: [${context.sha.slice(0, 7)}](https://github.com/${repo.owner}/${repo.repo}/commit/${context.sha})`,
    `Build: [${build}](https://github.com/${repo.owner}/${repo.repo}/actions/runs/${context.runId})`,
    'ARM64 · Android 8.0 or newer · Signed release APK',
    '',
    'iOS 27+: experimental wireless pairing is available from Developer Mode with both phones on the same Wi-Fi. Select iDroidLoader on iPhone and enter the code shown on Android.',
    'For earlier iOS versions, import a trusted Lockdown or iloader combined pairing file and enable iPhone Wi-Fi debugging first. Exported RemotePairing records can also be imported.',
    'Physical iPhone connection, Apple ID signing, and IPA installation still require real-device validation.',
    '',
    `[Instructions](https://github.com/${repo.owner}/${repo.repo}/blob/main/ANDROID.md)`,
  ].join('\n');

  let release;
  try {
    release = (await github.rest.repos.getReleaseByTag({ ...repo, tag })).data;
  } catch (error) {
    if (error.status !== 404) throw error;
    // The tag endpoint can omit unpublished drafts left by a failed first build.
    const releases = await github.paginate(github.rest.repos.listReleases, { ...repo, per_page: 100 });
    release = releases.find(candidate => candidate.tag_name === tag);
    if (!release) {
      release = (await github.rest.repos.createRelease({
        ...repo, tag_name: tag, target_commitish: context.sha,
        name, body, prerelease: nightly, draft: true, make_latest: 'false',
      })).data;
    }
  }

  // Upload all replacements before removing any currently downloadable asset.
  const uploaded = [];
  try {
    for (const name of files) {
      const data = readFile(path.join('artifacts', name));
      const asset = await github.rest.repos.uploadReleaseAsset({
        ...repo, release_id: release.id, name: `${name}.upload-${context.runId}-${context.runAttempt}`,
        headers: { 'content-type': 'application/octet-stream', 'content-length': data.length }, data,
      });
      uploaded.push({ id: asset.data.id, name });
    }
  } catch (error) {
    for (const asset of uploaded) {
      await github.rest.repos.deleteReleaseAsset({ ...repo, asset_id: asset.id }).catch(() => {});
    }
    throw error;
  }

  const assets = await github.paginate(github.rest.repos.listReleaseAssets, { ...repo, release_id: release.id });
  for (const asset of assets.filter(asset => files.includes(asset.name))) {
    await github.rest.repos.deleteReleaseAsset({ ...repo, asset_id: asset.id });
  }
  for (const asset of uploaded) {
    await github.rest.repos.updateReleaseAsset({ ...repo, asset_id: asset.id, name: asset.name });
  }

  // Move only our rolling tag; stable tags are never rewritten.
  if (nightly) {
    let tagExists = true;
    try {
      await github.rest.git.getRef({ ...repo, ref: `tags/${tag}` });
    } catch (error) {
      if (error.status !== 404) throw error;
      tagExists = false;
    }
    if (tagExists) {
      await github.rest.git.updateRef({ ...repo, ref: `tags/${tag}`, sha: context.sha, force: true });
    } else {
      await github.rest.git.createRef({ ...repo, ref: `refs/tags/${tag}`, sha: context.sha });
    }
  }
  await github.rest.repos.updateRelease({
    ...repo, release_id: release.id, name, body,
    prerelease: nightly, draft: false, make_latest: nightly ? 'false' : 'legacy',
  });
  core.notice(`Updated https://github.com/${repo.owner}/${repo.repo}/releases/tag/${tag}`);
};
