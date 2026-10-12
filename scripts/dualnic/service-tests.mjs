import { execFileSync } from 'node:child_process'
import path from 'node:path'
const root = path.resolve(import.meta.dirname, '../..')
const cargo = process.env.DUALNIC_CARGO || 'cargo'
const common = [
  'test',
  '--release',
  '--locked',
  '--manifest-path',
  'vendor/service-ipc/Cargo.toml',
  '--no-default-features',
  '--features',
  'client,standalone,dual-nic-channel',
]
for (const filter of [
  'dual_nic',
  'channel::tests::compiled_channel',
  'management::tests',
]) {
  execFileSync(cargo, [...common, '--lib', filter], {
    cwd: root,
    stdio: 'inherit',
  })
}
execFileSync(
  cargo,
  [...common, '--bin', 'clash-verge-dualnic-service-install'],
  { cwd: root, stdio: 'inherit' },
)
