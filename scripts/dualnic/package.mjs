import fs from 'node:fs/promises'
import path from 'node:path'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import AdmZip from 'adm-zip'

import { root, version } from './common.mjs'
const out =
  process.env.DUALNIC_PACKAGE_OUTPUT || path.join(root, 'release', version)
const target = process.env.CARGO_TARGET_DIR || path.join(root, 'target')
const prefix = 'ClashVergeDualNic-' + version + '-windows-x64'
const clientBinary = path.join(target, 'release/clash-verge-dualnic.exe')
const installer = path.join(
  target,
  'release/bundle/nsis/Clash Verge Dual NIC_' + version + '_x64-setup.exe',
)
for (const executable of [clientBinary, installer]) {
  const actual = execFileSync(
    path.join(
      process.env.SystemRoot,
      'System32/WindowsPowerShell/v1.0/powershell.exe',
    ),
    [
      '-NoProfile',
      '-NonInteractive',
      '-Command',
      '(Get-Item -LiteralPath $env:DUALNIC_VERSION_BINARY).VersionInfo.ProductVersion',
    ],
    {
      encoding: 'utf8',
      windowsHide: true,
      env: { ...process.env, DUALNIC_VERSION_BINARY: executable },
    },
  ).trim()
  if (actual !== version)
    throw new Error(
      'Stale binary version: ' +
        executable +
        ' has ' +
        actual +
        ', expected ' +
        version,
    )
}

const sha = (data) => createHash('sha256').update(data).digest('hex')
await fs.mkdir(out, { recursive: true })
const lock = JSON.parse(
  await fs.readFile(path.join(root, 'dualnic-build-lock.json'), 'utf8'),
)
const results = []
const inventory = {}
const put = async (zip, source, name) => {
  const data = await fs.readFile(source)
  zip.addFile(name, data)
  return { path: name, size: data.length, sha256: sha(data) }
}
const archive = async (zip, name, files) => {
  const file = path.join(out, name)
  zip.writeZip(file)
  // Reopen and read every entry to check archive data, paths and CRCs.
  const reopened = new AdmZip(file)
  for (const entry of reopened.getEntries()) {
    if (/^[/\\]|^[A-Za-z]:|(?:^|\/)\.\.(?:\/|$)/.test(entry.entryName))
      throw new Error('Unsafe archive path')
    if (!entry.isDirectory) entry.getData()
  }
  const data = await fs.readFile(file)
  inventory[name] = files
  results.push({
    file: name,
    bytes: data.length,
    sha256: sha(data),
    entries: files.length,
  })
  console.log(
    'Created ' + name + ' (' + (data.length / 1048576).toFixed(2) + ' MiB)',
  )
}
const portable = new AdmZip()
const portableFiles = []
portableFiles.push(await put(portable, clientBinary, 'clash-verge-dualnic.exe'))
for (const name of ['verge-mihomo-dualnic', 'verge-mihomo-dualnic-alpha']) {
  const source = path.join(
    root,
    'src-tauri/sidecar',
    name + '-x86_64-pc-windows-msvc.exe',
  )
  if (sha(await fs.readFile(source)) !== lock.mihomo.sha256)
    throw new Error('Wrong bundled Mihomo')
  portableFiles.push(await put(portable, source, name + '.exe'))
}
for (const name of await fs.readdir(path.join(root, 'src-tauri/resources'))) {
  const source = path.join(root, 'src-tauri/resources', name)
  if (!(await fs.lstat(source)).isFile())
    throw new Error('Unexpected resource directory')
  portableFiles.push(await put(portable, source, 'resources/' + name))
}
for (const name of ['README.md', 'LICENSE', 'dualnic-build-lock.json'])
  portableFiles.push(await put(portable, path.join(root, name), name))
for (const name of [
  'usage.md',
  'acceptance.md',
  'manual-test.md',
  'license-review.md',
]) {
  const rel = 'docs/dualnic/' + name
  portableFiles.push(await put(portable, path.join(root, rel), rel))
}
portableFiles.push(
  await put(
    portable,
    path.join(root, 'third-party/LICENSE-Mihomo.txt'),
    'third-party/LICENSE-Mihomo.txt',
  ),
)
await archive(portable, prefix + '-portable.zip', portableFiles)

const sourceZip = new AdmZip()
const sourceFiles = []
const candidates = new Set(
  execFileSync(
    'git',
    ['ls-files', '-z', '--cached', '--others', '--exclude-standard'],
    { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 },
  )
    .split('\0')
    .filter(Boolean),
)
candidates.add('vendor/service-ipc/Cargo.lock')
for (const rel of [...candidates].sort()) {
  if (
    /^(?:node_modules|target|dist|release|src-tauri\/resources|src-tauri\/sidecar)\//.test(
      rel,
    ) ||
    /(?:^|\/)(?:\.git|\.codex|\.agents)(?:\/|$)/.test(rel)
  )
    continue
  const absolute = path.resolve(root, rel)
  if (
    !absolute.startsWith(root + path.sep) ||
    !(await fs.lstat(absolute).catch(() => null))?.isFile()
  )
    continue
  sourceFiles.push(await put(sourceZip, absolute, rel))
}
await archive(sourceZip, prefix + '-source.zip', sourceFiles)

const resourcesZip = new AdmZip()
const resourcesFiles = []
for (const name of ['verge-mihomo-dualnic', 'verge-mihomo-dualnic-alpha']) {
  const rel = 'src-tauri/sidecar/' + name + '-x86_64-pc-windows-msvc.exe'
  resourcesFiles.push(await put(resourcesZip, path.join(root, rel), rel))
}
for (const name of Object.keys(lock.assets)) {
  const rel = 'src-tauri/resources/' + name
  if (sha(await fs.readFile(path.join(root, rel))) !== lock.assets[name])
    throw new Error('Wrong asset: ' + name)
  resourcesFiles.push(await put(resourcesZip, path.join(root, rel), rel))
}
await archive(resourcesZip, prefix + '-build-resources.zip', resourcesFiles)

const installerData = await fs.readFile(installer)
const installerName = prefix + '-setup.exe'
await fs.writeFile(path.join(out, installerName), installerData)
results.push({
  file: installerName,
  bytes: installerData.length,
  sha256: sha(installerData),
})
const coreSource = path.join(root, 'third-party/mihomo-v1.19.32-source.zip')
{
  if (sha(await fs.readFile(coreSource)) !== lock.mihomo.source_sha256)
    throw new Error('Missing or incorrect corresponding Mihomo source archive')
  const data = await fs.readFile(coreSource)
  const name = 'Mihomo-v1.19.32-source.zip'
  await fs.writeFile(path.join(out, name), data)
  results.push({ file: name, bytes: data.length, sha256: sha(data) })
}
await fs.writeFile(
  path.join(out, 'SHA256SUMS.txt'),
  results.map((r) => r.sha256 + '  ' + r.file).join('\n') + '\n',
)
await fs.writeFile(
  path.join(out, 'package-inventory.json'),
  JSON.stringify(
    {
      createdAt: new Date().toISOString(),
      version,
      upstream: lock,
      packages: results,
      inventory,
    },
    null,
    2,
  ) + '\n',
)
console.log(JSON.stringify(results, null, 2))
