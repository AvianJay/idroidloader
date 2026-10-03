const test = require('node:test');
const assert = require('node:assert/strict');
const publish = require('../.github/scripts/publish-android.cjs');

const context = {
  repo: { owner: 'AvianJay', repo: 'idroidloader' },
  sha: 'first-commit', runId: 101, runNumber: 1, runAttempt: 1,
};

function fixture({ existing = false, draft = false, failUpload = 0, denied = false, stale = false, channel = 'nightly' } = {}) {
  const releaseTag = channel === 'nightly' ? 'nightly' : 'v2.3.4';
  const state = {
    head: stale ? 'newer-commit' : context.sha,
    release: existing ? { id: 17, tag_name: 'nightly', draft } : null,
    creates: 0, updates: [], uploads: 0, tag: null, refs: [], notices: [],
    assets: existing ? [
      { id: 1, name: 'iDroidLoader-arm64.apk' },
      { id: 2, name: 'iDroidLoader-arm64.apk.sha256' },
      { id: 3, name: 'unrelated.txt' },
    ] : [],
  };
  const notFound = () => Object.assign(new Error('Not found'), { status: 404 });
  const repos = {
    getReleaseByTag: async ({ tag }) => {
      assert.equal(tag, releaseTag);
      if (denied) throw Object.assign(new Error('Forbidden'), { status: 403 });
      if (!state.release || state.release.draft) throw notFound();
      return { data: state.release };
    },
    listReleases: async () => ({ data: state.release ? [state.release] : [] }),
    createRelease: async args => {
      state.creates++;
      state.release = { ...args, id: 17 };
      return { data: state.release };
    },
    listReleaseAssets: async () => ({ data: state.assets.slice() }),
    uploadReleaseAsset: async args => {
      assert.equal(args.release_id, 17);
      state.uploads++;
      if (state.uploads === failUpload) throw new Error('Upload failed');
      const asset = { id: 100 + state.uploads, name: args.name };
      state.assets.push(asset);
      return { data: asset };
    },
    deleteReleaseAsset: async ({ asset_id }) => {
      state.assets = state.assets.filter(asset => asset.id !== asset_id);
    },
    updateReleaseAsset: async ({ asset_id, name }) => {
      state.assets.find(asset => asset.id === asset_id).name = name;
    },
    updateRelease: async args => {
      assert.equal(args.release_id, state.release.id);
      state.updates.push(args);
      Object.assign(state.release, args);
    },
  };
  const github = {
    rest: { repos, git: {
      getRef: async ({ ref }) => {
        if (ref === 'tags/nightly') {
          if (!state.tag) throw notFound();
          return { data: { object: { sha: state.tag } } };
        }
        assert.equal(ref, 'heads/main');
        return { data: { object: { sha: state.head } } };
      },
      updateRef: async args => {
        // GitHub returns 422, rather than 404, when PATCH targets a missing ref.
        if (!state.tag) throw Object.assign(new Error('Reference does not exist'), { status: 422 });
        state.refs.push(args);
        state.tag = args.sha;
      },
      createRef: async args => {
        state.refs.push(args);
        state.tag = args.sha;
      },
    } },
    paginate: async (method, args) => (await method(args)).data,
  };
  return {
    state, github, channel, context: { ...context, ref: `refs/tags/${releaseTag}` },
    core: { notice: text => state.notices.push(text) },
    readFile: file => Buffer.from(file.endsWith('.json')
      ? JSON.stringify({ channel, version: channel === 'nightly' ? '2.3.4-nightly.1' : '2.3.4' })
      : 'public artifact fixture'),
  };
}

test('creates one nightly release and reuses its ID on the next build', async () => {
  const setup = fixture();
  await publish(setup);
  setup.context.sha = setup.state.head = 'second-commit';
  setup.context.runId = 102;
  await publish(setup);
  assert.equal(setup.state.creates, 1);
  assert.equal(setup.state.updates.length, 2);
  assert.equal(setup.state.release.draft, false);
  assert.equal(setup.state.release.prerelease, true);
  assert.equal(setup.state.tag, 'second-commit');
  assert.deepEqual(setup.state.assets.map(asset => asset.name).sort(), [
    'android-update.json', 'iDroidLoader-arm64.apk', 'iDroidLoader-arm64.apk.sha256',
  ]);
  assert.deepEqual(setup.state.refs.map(ref => ref.ref), ['refs/tags/nightly', 'tags/nightly']);
});

test('reuses a draft left by an interrupted first publication', async () => {
  const setup = fixture({ existing: true, draft: true });
  await publish(setup);
  assert.equal(setup.state.creates, 0);
  assert.equal(setup.state.release.id, 17);
  assert.equal(setup.state.release.draft, false);
  assert.ok(setup.state.assets.some(asset => asset.name === 'unrelated.txt'));
});

test('failed replacement upload preserves both published assets and removes the partial upload', async () => {
  const setup = fixture({ existing: true, failUpload: 2 });
  await assert.rejects(publish(setup), /Upload failed/);
  assert.deepEqual(setup.state.assets.map(asset => asset.id), [1, 2, 3]);
  assert.equal(setup.state.updates.length, 0);
  assert.equal(setup.state.refs.length, 0);
});

test('does not create another release when the tag lookup fails with an authorization error', async () => {
  const setup = fixture({ denied: true });
  await assert.rejects(publish(setup), /Forbidden/);
  assert.equal(setup.state.creates, 0);
  assert.equal(setup.state.uploads, 0);
});

test('does not overwrite nightly with an obsolete commit', async () => {
  const setup = fixture({ stale: true });
  await publish(setup);
  assert.equal(setup.state.creates, 0);
  assert.equal(setup.state.uploads, 0);
  assert.equal(setup.state.notices.length, 1);
});

test('publishes stable APKs and updater metadata without moving the release tag', async () => {
  const setup = fixture({ channel: 'release' });
  await publish(setup);
  assert.equal(setup.state.release.tag_name, 'v2.3.4');
  assert.equal(setup.state.release.prerelease, false);
  assert.equal(setup.state.release.make_latest, 'legacy');
  assert.equal(setup.state.refs.length, 0);
  assert.ok(setup.state.assets.some(asset => asset.name === 'android-update.json'));
});

test('rejects stable tags that do not match the packaged version before publishing', async () => {
  const setup = fixture({ channel: 'release' });
  setup.context.ref = 'refs/tags/v2.4.0';
  await assert.rejects(publish(setup), /metadata does not match/);
  assert.equal(setup.state.creates, 0);
  assert.equal(setup.state.uploads, 0);
});

test('failure uploading metadata preserves the previously published APKs', async () => {
  const setup = fixture({ existing: true, failUpload: 3 });
  await assert.rejects(publish(setup), /Upload failed/);
  assert.deepEqual(setup.state.assets.map(asset => asset.id), [1, 2, 3]);
  assert.equal(setup.state.updates.length, 0);
});
