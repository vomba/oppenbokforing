import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import test from 'node:test';
import { buildPdfHelper, createBuildPlan } from './build-pdf-helper.mjs';

const arm = 'aarch64-apple-darwin';
const intel = 'x86_64-apple-darwin';
const universal = 'universal-apple-darwin';
const root = '/project';
const targetDirectory = '/artifacts';
const staged = target => join(root, 'src-tauri', 'binaries', `oppenbokforing-pdf-helper-${target}`);
const built = (target, profile = 'debug') => join(targetDirectory, target, profile, 'oppenbokforing-pdf-helper');
const sibling = profile => join(targetDirectory, profile, 'oppenbokforing-pdf-helper');
const plan = overrides => createBuildPlan({ host: arm, platform: 'darwin', env: {}, args: [], root, targetDirectory, ...overrides });

function execute({ host = arm, platform = 'darwin', env = {}, args = [], failBuild, failLipo, missingLipo = false } = {}) {
  const commands = [];
  const copies = [];
  const directories = [];
  const executables = [];
  const run = (tool, commandArgs) => {
    commands.push([tool, commandArgs]);
    if (tool === 'rustc') return `rustc 1.99\nhost: ${host}\n`;
    if (tool === 'cargo' && commandArgs[0] === 'metadata') return JSON.stringify({ target_directory: targetDirectory });
    if (tool === 'cargo' && failBuild === commandArgs[commandArgs.indexOf('--target') + 1]) throw new Error("can't find crate for std");
    if (tool === '/usr/bin/lipo' && failLipo === commandArgs[0]) throw new Error('lipo rejected architectures');
    return '';
  };
  const options = {
    root, platform, env, args, run,
    copyFile: (source, destination) => copies.push([source, destination]),
    mkdir: (directory, options) => directories.push([directory, options]),
    checkExecutable: path => { executables.push(path); if (missingLipo) throw new Error('ENOENT'); },
    log: () => {},
  };
  return { run: () => buildPdfHelper(options), commands, copies, directories, executables };
}

test('import is inert and does not invoke Cargo, even with no tools on PATH', () => {
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', `import ${JSON.stringify(new URL('./build-pdf-helper.mjs', import.meta.url).href)}`], {
    encoding: 'utf8', env: { ...process.env, PATH: '' },
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, '');
  assert.equal(result.stderr, '');
});

for (const host of [arm, intel, 'x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc']) {
  for (const release of [false, true]) {
    test(`native ${host} ${release ? 'release' : 'debug'} stages explicit target and developer sibling`, () => {
      const execution = execute({ host, platform: host.includes('darwin') ? 'darwin' : host.includes('windows') ? 'win32' : 'linux', args: release ? ['--release'] : [] });
      const result = execution.run();
      const profile = release ? 'release' : 'debug';
      assert.equal(result.profile, profile);
      assert.equal(result.builds.length, 1);
      const build = result.builds[0];
      assert.equal(build.target, host);
      assert.deepEqual(build.args.slice(0, 7), ['build', '--manifest-path', '/project/src-tauri/Cargo.toml', '--no-default-features', '--target', host, '--bin']);
      assert.equal(build.args.includes('--release'), release);
      const suffix = host.includes('windows') ? '.exe' : '';
      assert.equal(build.built, `${built(host, profile)}${suffix}`);
      assert.deepEqual(execution.copies, [[build.built, `${staged(host)}${suffix}`], [build.built, `${sibling(profile)}${suffix}`]]);
      assert.equal(execution.executables.length, 0);
    });
  }
}

test('official Tauri hooks: debug=true is debug; absent DEBUG with TARGET_TRIPLE is release', () => {
  assert.equal(plan({ env: { TAURI_ENV_TARGET_TRIPLE: arm, TAURI_ENV_DEBUG: 'true' } }).profile, 'debug');
  assert.equal(plan({ env: { TAURI_ENV_TARGET_TRIPLE: arm } }).profile, 'release');
  assert.equal(plan({ env: { TAURI_ENV_DEBUG: 'false' } }).profile, 'release');
  assert.equal(plan({ args: ['--release'], env: { TAURI_ENV_DEBUG: 'true' } }).profile, 'release');
});

for (const host of [arm, intel]) {
  test(`universal release on ${host} builds and stages BOTH targets, then lipo creates and verifies both architectures`, () => {
    const execution = execute({ host, env: { TAURI_ENV_TARGET_TRIPLE: universal } });
    const result = execution.run();
    assert.deepEqual(result.builds.map(build => build.target), [arm, intel]);
    const cargoBuilds = execution.commands.filter(([tool, args]) => tool === 'cargo' && args[0] === 'build');
    assert.equal(cargoBuilds.length, 2);
    assert.deepEqual(cargoBuilds.map(([, args]) => args[args.indexOf('--target') + 1]), [arm, intel]);
    assert.ok(cargoBuilds.every(([, args]) => args.includes('--release')));
    assert.ok(!cargoBuilds.some(([, args]) => args.includes(universal)));
    assert.deepEqual(execution.copies, [
      [built(arm, 'release'), staged(arm)],
      ...(host === arm ? [[built(arm, 'release'), sibling('release')]] : []),
      [built(intel, 'release'), staged(intel)],
      ...(host === intel ? [[built(intel, 'release'), sibling('release')]] : []),
    ]);
    assert.deepEqual(execution.commands.slice(-2), [
      ['/usr/bin/lipo', ['-create', staged(arm), staged(intel), '-output', staged(universal)]],
      ['/usr/bin/lipo', ['-verify_arch', 'arm64', 'x86_64', staged(universal)]],
    ]);
    assert.deepEqual(execution.executables, ['/usr/bin/lipo']);
  });
}

test('universal debug builds use debug artifacts, not a stale release helper', () => {
  const execution = execute({ env: { TAURI_ENV_TARGET_TRIPLE: universal, TAURI_ENV_DEBUG: 'true' } });
  assert.equal(execution.run().profile, 'debug');
  assert.ok(execution.copies.every(([source]) => source.includes('/debug/')));
});

for (const [host, target] of [[arm, intel], [intel, arm]]) {
  test(`explicit macOS cross ${host} -> ${target} never overwrites the host sibling or substitutes host output`, () => {
    const execution = execute({ host, env: { TAURI_ENV_TARGET_TRIPLE: target, TAURI_ENV_DEBUG: 'true', CARGO_BUILD_TARGET: target } });
    const result = execution.run();
    assert.equal(result.builds.length, 1);
    assert.equal(result.builds[0].target, target);
    assert.equal(result.builds[0].nativeSibling, undefined);
    assert.deepEqual(execution.copies, [[built(target), staged(target)]]);
    assert.ok(!execution.commands.some(([tool]) => tool === '/usr/bin/lipo'));
  });
}

for (const input of [
  { host: 'x86_64-unknown-linux-gnu', platform: 'linux', target: arm },
  { host: 'x86_64-unknown-linux-gnu', platform: 'linux', target: universal },
  { host: 'x86_64-unknown-linux-gnu', platform: 'linux', target: 'aarch64-unknown-linux-gnu' },
  { host: arm, platform: 'darwin', target: 'x86_64-pc-windows-msvc' },
]) {
  test(`unsupported cross ${input.host} -> ${input.target} fails before metadata/build/staging`, () => {
    const execution = execute({ ...input, env: { TAURI_ENV_TARGET_TRIPLE: input.target } });
    assert.throws(execution.run, /cross target .* is not supported/);
    assert.equal(execution.commands.length, 1);
    assert.deepEqual(execution.copies, []);
  });
}

test('target configuration mismatch fails before Cargo, not silently substituted with host', () => {
  for (const env of [
    { CARGO_BUILD_TARGET: intel },
    { TAURI_ENV_TARGET_TRIPLE: intel, CARGO_BUILD_TARGET: arm },
    { TAURI_ENV_TARGET_TRIPLE: universal, CARGO_BUILD_TARGET: arm },
    { TAURI_ENV_TARGET_TRIPLE: universal, CARGO_BUILD_TARGET: universal },
  ]) {
    const execution = execute({ env });
    assert.throws(execution.run, /CARGO_BUILD_TARGET/);
    assert.equal(execution.commands.length, 1);
    assert.deepEqual(execution.copies, []);
  }
});

test('invalid arguments/debug or absent Rust host fail clearly', () => {
  assert.throws(() => plan({ args: ['--target', intel] }), /Only --release/);
  assert.throws(() => plan({ env: { TAURI_ENV_DEBUG: '1' } }), /Unexpected TAURI_ENV_DEBUG/);
  assert.throws(() => plan({ host: undefined }), /Cannot determine Rust host/);
});

test('missing lipo fails before builds; no automatic installation', () => {
  const execution = execute({ env: { TAURI_ENV_TARGET_TRIPLE: universal }, missingLipo: true });
  assert.throws(execution.run, /requires existing executable \/usr\/bin\/lipo.*no tools are installed automatically/);
  assert.ok(!execution.commands.some(([, args]) => args[0] === 'build'));
  assert.deepEqual(execution.copies, []);
});

test('missing cross Rust target fails honestly without staging/substituting host output', () => {
  const execution = execute({ env: { TAURI_ENV_TARGET_TRIPLE: intel }, failBuild: intel });
  assert.throws(execution.run, /Cargo build for x86_64-apple-darwin failed.*already installed/);
  assert.deepEqual(execution.copies, []);
});

test('second universal target failure never falls back to host or runs lipo', () => {
  const execution = execute({ env: { TAURI_ENV_TARGET_TRIPLE: universal }, failBuild: intel });
  assert.throws(execution.run, /Cargo build for x86_64-apple-darwin failed/);
  assert.ok(!execution.copies.some(([, destination]) => destination === staged(intel) || destination === staged(universal)));
  assert.ok(!execution.commands.some(([tool]) => tool === '/usr/bin/lipo'));
});

for (const failLipo of ['-create', '-verify_arch']) {
  test(`lipo ${failLipo} failure is not reported as successful universal staging`, () => {
    const execution = execute({ env: { TAURI_ENV_TARGET_TRIPLE: universal }, failLipo });
    assert.throws(execution.run, /lipo rejected architectures/);
  });
}
