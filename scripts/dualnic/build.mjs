import path from 'node:path'
import fs from 'node:fs'
import { execFileSync } from 'node:child_process'
import { root } from './common.mjs'
const env = {
  ...process.env,
  DUALNIC_NSIS_PLUGIN_DIR:
    process.env.DUALNIC_NSIS_PLUGIN_DIR ||
    path.join(root, 'src-tauri/packages/windows/plugins'),
}
const cargo = env.DUALNIC_CARGO || 'cargo'
const node = process.execPath
const run = (exe, args) =>
  execFileSync(exe, args, { cwd: root, env, stdio: 'inherit' })
run(node, ['scripts/dualnic/isolation-check.mjs'])
run(node, ['scripts/dualnic/prebuild.mjs'])
run(node, ['scripts/dualnic/web-build.mjs'])
run(cargo, [
  'build',
  '--release',
  '--locked',
  '-p',
  'clash-verge-dualnic-app',
  '--bins',
])
const target = env.CARGO_TARGET_DIR || path.join(root, 'target')
if (fs.existsSync(path.join(target, 'release/frontend-contract.exe'))) {
  env.DUALNIC_CONTRACT_BINARY = path.join(
    target,
    'release/frontend-contract.exe',
  )
  const contract = path.join(root, 'src/services/contract.ts')
  const before = fs.readFileSync(contract, 'utf8')
  run(node, ['scripts/gen-contract.mjs'])
  if (before !== fs.readFileSync(contract, 'utf8')) {
    run(node, ['scripts/dualnic/web-build.mjs'])
    run(cargo, [
      'build',
      '--release',
      '--locked',
      '-p',
      'clash-verge-dualnic-app',
      '--bin',
      'clash-verge-dualnic',
    ])
  } else {
    console.log(
      'Frontend contract unchanged; reusing the production frontend and client.',
    )
  }
}
run(node, [
  'node_modules/@tauri-apps/cli/tauri.js',
  'bundle',
  '--bundles',
  'nsis',
  '--ci',
  '--no-sign',
])
