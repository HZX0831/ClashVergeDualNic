import fs from 'node:fs/promises'
import path from 'node:path'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'

// This fork intentionally accepts only attested, fixed resources. Never fetch latest/Alpha.
const root = path.resolve(import.meta.dirname, '../..')
const lock = JSON.parse(
  await fs.readFile(path.join(root, 'dualnic-build-lock.json'), 'utf8'),
)
if (process.platform !== 'win32' || process.arch !== 'x64')
  throw new Error('Windows x64 is the supported build target')
const digest = async (file) =>
  createHash('sha256')
    .update(await fs.readFile(file))
    .digest('hex')
const resources = path.join(root, 'src-tauri/resources')
const sidecar = path.join(root, 'src-tauri/sidecar')
await fs.mkdir(resources, { recursive: true })
await fs.mkdir(sidecar, { recursive: true })
const core =
  process.env.DUALNIC_MIHOMO_SOURCE ||
  path.join(sidecar, 'verge-mihomo-dualnic-x86_64-pc-windows-msvc.exe')
if ((await digest(core)) !== lock.mihomo.sha256)
  throw new Error('Mihomo resource hash differs from the fixed build lock')
if (
  !/Mihomo Meta v1\.19\.32(?:\s|$)/.test(
    execFileSync(core, ['-v'], { encoding: 'utf8' }),
  )
)
  throw new Error('Mihomo must be 1.19.32')
for (const name of ['verge-mihomo-dualnic', 'verge-mihomo-dualnic-alpha']) {
  const target = path.join(sidecar, name + '-x86_64-pc-windows-msvc.exe')
  if (path.resolve(core) !== path.resolve(target))
    await fs.copyFile(core, target)
}
for (const [name, hash] of Object.entries(lock.assets)) {
  const target = path.join(resources, name)
  if (process.env.DUALNIC_ASSETS_SOURCE)
    await fs.copyFile(
      path.join(process.env.DUALNIC_ASSETS_SOURCE, name),
      target,
    )
  if ((await digest(target)) !== hash)
    throw new Error('Asset hash mismatch: ' + name)
}
const pluginRoot = path.resolve(
  process.env.DUALNIC_NSIS_PLUGIN_DIR ||
    path.join(root, 'src-tauri/packages/windows/plugins'),
)
const plugin = path.join(pluginRoot, 'SimpleSC.dll')
if (
  (await digest(plugin)) !==
  '1620cdf739f459d1d83411f93648f29dcf947a910cc761e85ac79a69639d127d'
)
  throw new Error('NSIS SimpleSC 1.30 plugin hash mismatch')
const cargo = process.env.DUALNIC_CARGO || 'cargo'
const serviceTarget = path.join(root, 'target/bundled-service')
execFileSync(
  cargo,
  [
    'build',
    '--locked',
    '--manifest-path',
    path.join(root, 'vendor/service-ipc/Cargo.toml'),
    '--target-dir',
    serviceTarget,
    '--target',
    'x86_64-pc-windows-msvc',
    '--release',
    '--features',
    'standalone,client,dual-nic-channel',
    '--bins',
  ],
  { cwd: root, stdio: 'inherit' },
)
for (const name of [
  'clash-verge-dualnic-service',
  'clash-verge-dualnic-service-install',
  'clash-verge-dualnic-service-uninstall',
])
  await fs.copyFile(
    path.join(serviceTarget, 'x86_64-pc-windows-msvc/release', name + '.exe'),
    path.join(resources, name + '.exe'),
  )
await fs.writeFile(
  path.join(root, 'src-tauri/packages/windows/core-hashes.nsh'),
  '!define MIHOMO_SHA256 "' +
    lock.mihomo.sha256 +
    '"\n!define MIHOMO_ALPHA_SHA256 "' +
    lock.mihomo.sha256 +
    '"\n',
)
console.log('Fixed Mihomo, isolated service and resource hashes prepared.')

// Include the same license and usage notices in installer and portable resources.
for (const [source, name] of [
  ['LICENSE', 'LICENSE-client.txt'],
  ['vendor/service-ipc/LICENSE', 'LICENSE-service.txt'],
  [
    'src-tauri/packages/windows/plugins/LICENSE-SimpleSC.txt',
    'LICENSE-SimpleSC.txt',
  ],
  ['README.md', 'README.md'],
  ['docs/dualnic/usage.md', 'dual-nic-usage.md'],
  ['docs/dualnic/license-review.md', 'license-review.md'],
  ['third-party/LICENSE-Mihomo.txt', 'LICENSE-Mihomo.txt'],
  ['docs/dualnic/acceptance.md', 'dual-nic-acceptance.md'],
  ['docs/dualnic/manual-test.md', 'dual-nic-manual-test.md'],
  ['dualnic-build-lock.json', 'dualnic-build-lock.json'],
])
  await fs.copyFile(path.join(root, source), path.join(resources, name))
