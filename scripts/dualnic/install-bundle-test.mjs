import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import path from 'node:path'
import { execFileSync, spawnSync } from 'node:child_process'

const root = path.resolve(import.meta.dirname, '../..')
const target = process.env.CARGO_TARGET_DIR || path.join(root, 'target')
const bundle = process.env.DUALNIC_BUNDLE_DIR || path.join(root, 'src-tauri')
const installer = path.join(bundle, 'resources/clash-verge-dualnic-service-install.exe')
const core = process.env.DUALNIC_BUNDLE_DIR
  ? path.join(bundle, 'verge-mihomo-dualnic.exe')
  : path.join(bundle, 'sidecar/verge-mihomo-dualnic-x86_64-pc-windows-msvc.exe')
const options = { encoding: 'utf8', windowsHide: true, timeout: 20000 }
const args = (source) => [
  '--prepare-install', '--check-only', '--core', 'verge-mihomo-dualnic.exe', source,
]
const output = execFileSync(installer, args(core), options)
assert.match(output, /Service installation bundle verified/)
assert.match(output, /clash-verge-dualnic-service\.exe/)

const fixture = await fs.mkdtemp(path.join(target, 'service-bundle-'))
try {
  const directory = path.join(fixture, '中文目录 with spaces')
  await fs.mkdir(directory)
  const tool = path.join(directory, path.basename(installer))
  await fs.copyFile(installer, tool)
  await fs.copyFile(path.join(bundle, 'resources/clash-verge-dualnic-service.exe'), path.join(directory, 'clash-verge-dualnic-service.exe'))
  const fixtureCore = path.join(directory, 'verge-mihomo-dualnic.exe')
  await fs.writeFile(fixtureCore, 'bundle preparation fixture')
  assert.match(execFileSync(tool, args(fixtureCore), options), /Service installation bundle verified/)
  await fs.rename(path.join(directory, 'clash-verge-dualnic-service.exe'), path.join(directory, 'clash-verge-service.exe'))
  const rejected = spawnSync(tool, args(fixtureCore), options)
  assert.notEqual(rejected.status, 0)
  assert.match(rejected.stderr, /os error 2/)
  console.log('Native installation preparation passed for the real bundle and a Chinese path; official-name substitution was rejected. No service registration or elevation was performed.')
} finally {
  if (path.dirname(path.resolve(fixture)) !== path.resolve(target))
    throw new Error('Unexpected fixture directory')
  await fs.rm(fixture, { recursive: true, force: true })
}
