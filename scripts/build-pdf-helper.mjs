// Build before Tauri's own Cargo invocation (never recursively from build.rs).
// No library/tool installation: existing Rust toolchain, /usr/bin/lipo + Node builtins.
import { execFileSync } from 'node:child_process';
import { accessSync, constants, copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptPath = fileURLToPath(import.meta.url);
const projectRoot = resolve(dirname(scriptPath), '..');
const helper = 'oppenbokforing-pdf-helper';
const macTargets = ['aarch64-apple-darwin', 'x86_64-apple-darwin'];
const universalTarget = 'universal-apple-darwin';
const lipo = '/usr/bin/lipo';

// Pure planning keeps target/profile selection testable without running Cargo.
export function createBuildPlan({ host, platform, env = {}, args = [], root, targetDirectory }) {
  if (args.some(arg => arg !== '--release')) throw new Error('Only --release is supported; target comes from TAURI_ENV_TARGET_TRIPLE.');
  if (!host) throw new Error('Cannot determine Rust host target.');
  const target = env.TAURI_ENV_TARGET_TRIPLE || host;
  const universal = target === universalTarget;
  const macBuild = platform === 'darwin' && macTargets.includes(host)
    && (universal || macTargets.includes(target));
  if (target !== host && !macBuild) {
    throw new Error(`PDF helper cross target ${target} is not supported on ${platform} (host ${host}). Only native builds and the macOS architecture pair on Darwin are supported.`);
  }
  if (env.CARGO_BUILD_TARGET && env.CARGO_BUILD_TARGET !== target) {
    throw new Error(`PDF helper target ${target} conflicts with CARGO_BUILD_TARGET=${env.CARGO_BUILD_TARGET}; unset it or select the same target.`);
  }
  if (universal && env.CARGO_BUILD_TARGET) {
    throw new Error('Unset CARGO_BUILD_TARGET for universal builds; Cargo must build each real macOS target separately.');
  }
  const debug = env.TAURI_ENV_DEBUG;
  if (debug !== undefined && debug !== 'true' && debug !== 'false') throw new Error(`Unexpected TAURI_ENV_DEBUG=${debug}`);
  // Tauri CLI sets DEBUG=true for debug hooks, but omits DEBUG for release hooks.
  // TARGET_TRIPLE is always supplied by the official hooks. Standalone stays debug.
  const release = args.includes('--release') || debug === 'false'
    || (Boolean(env.TAURI_ENV_TARGET_TRIPLE) && debug === undefined);
  const profile = release ? 'release' : 'debug';
  const targets = universal ? macTargets : [target];
  const manifest = join(root, 'src-tauri', 'Cargo.toml');
  const builds = targets.map(buildTarget => {
    const suffix = buildTarget.includes('windows') ? '.exe' : '';
    const name = `${helper}${suffix}`;
    return {
      target: buildTarget,
      args: ['build', '--manifest-path', manifest, '--no-default-features', '--target', buildTarget, '--bin', helper, ...(release ? ['--release'] : [])],
      built: join(targetDirectory, buildTarget, profile, name),
      staged: join(root, 'src-tauri', 'binaries', `${helper}-${buildTarget}${suffix}`),
      // Never overwrite a host developer sibling with a cross-architecture binary.
      nativeSibling: buildTarget === host ? join(targetDirectory, profile, name) : undefined,
    };
  });
  const combined = universal ? join(root, 'src-tauri', 'binaries', `${helper}-${universalTarget}`) : undefined;
  return { target, profile, builds, combined };
}

export function buildPdfHelper({
  root = projectRoot, env = process.env, args = process.argv.slice(2), platform = process.platform,
  run = (tool, toolArgs) => execFileSync(tool, toolArgs, {
    cwd: root, env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'],
  }).trim(),
  copyFile = copyFileSync, mkdir = mkdirSync,
  checkExecutable = path => accessSync(path, constants.X_OK), log = console.log,
} = {}) {
  const host = run('rustc', ['-vV']).match(/^host: (.+)$/m)?.[1];
  // Validate targets before metadata or any build/staging work.
  const input = { host, platform, env, args, root };
  createBuildPlan({ ...input, targetDirectory: '' });
  const metadata = JSON.parse(run('cargo', ['metadata', '--manifest-path', join(root, 'src-tauri', 'Cargo.toml'), '--no-deps', '--format-version', '1']));
  const plan = createBuildPlan({ ...input, targetDirectory: metadata.target_directory });
  if (plan.combined) {
    try {
      checkExecutable(lipo);
    } catch (error) {
      throw new Error(`Universal PDF helper requires existing executable ${lipo}; no tools are installed automatically. ${error.message}`);
    }
  }
  function copy(source, destination) {
    mkdir(dirname(destination), { recursive: true });
    copyFile(source, destination);
  }
  for (const build of plan.builds) {
    try {
      run('cargo', build.args);
    } catch (error) {
      throw new Error(`PDF helper Cargo build for ${build.target} failed. Ensure that Rust target and its linker are already installed (no automatic installation). ${error.message}`);
    }
    // tauri-build needs both architecture-suffixed files during universal compilation.
    copy(build.built, build.staged);
    if (build.nativeSibling) copy(build.built, build.nativeSibling);
    log(`PDF helper (${plan.profile}): ${build.built}\nTauri externalBin: ${build.staged}`);
  }
  if (plan.combined) {
    run(lipo, ['-create', ...plan.builds.map(build => build.staged), '-output', plan.combined]);
    run(lipo, ['-verify_arch', 'arm64', 'x86_64', plan.combined]);
    log(`Tauri universal externalBin (verified arm64 + x86_64): ${plan.combined}`);
  }
  return plan;
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  try {
    buildPdfHelper();
  } catch (error) {
    console.error(`PDF helper build failed: ${error.message}`);
    process.exitCode = 1;
  }
}
