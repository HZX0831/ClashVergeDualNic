import fs from 'node:fs'
import path from 'node:path'
import { execFileSync } from 'node:child_process'
if (process.platform !== 'win32')
  throw new Error('Windows x64 backend tests require the Windows SDK')
const root = path.resolve(import.meta.dirname, '../..')
const target = process.env.CARGO_TARGET_DIR || path.join(root, 'target')
const cargo = process.env.DUALNIC_CARGO || 'cargo'
const stdout = execFileSync(
  cargo,
  [
    'test',
    '--release',
    '--locked',
    '-p',
    'clash-verge-dualnic-app',
    '--lib',
    '--no-run',
    '--message-format=json',
  ],
  {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
    stdio: ['inherit', 'pipe', 'inherit'],
  },
)
const artifact = stdout
  .split('\n')
  .filter(Boolean)
  .map((s) => {
    try {
      return JSON.parse(s)
    } catch {
      return null
    }
  })
  .find(
    (a) =>
      a?.reason === 'compiler-artifact' &&
      a.target.name === 'app_lib' &&
      a.profile.test &&
      a.executable,
  )
if (!artifact) throw new Error('No backend unit-test executable was produced')
// Tauri embeds Common Controls v6 in the app, but not in its Rust test executable.
const manifest = path.join(target, 'windows-test-manifest.xml')
fs.writeFileSync(
  manifest,
  '<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0"><dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency></assembly>\n',
)
execFileSync(
  process.env.DUALNIC_MT || 'mt.exe',
  ['-manifest', manifest, '-outputresource:' + artifact.executable + ';#1'],
  { cwd: root, stdio: 'inherit' },
)
for (const filter of [
  'dual_nic_import::tests',
  'core::dual_nic::diagnostic_tests',
  'core::dual_nic::route_tests',
  'feat::executor::tests',
  'feat::effects::tests',
  'core::manager::config::tests',
  'core::manager::lifecycle::tests',
  'windows_pipe_name_tests',
])
  execFileSync(artifact.executable, [filter], { cwd: root, stdio: 'inherit' })
