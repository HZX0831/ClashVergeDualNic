import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { root } from './common.mjs'
const read = (p) => fs.readFileSync(path.join(root, p), 'utf8')
const nsis = read('src-tauri/packages/windows/installer.nsi')
for (const m of nsis.matchAll(
  /SimpleSC::(?:ExistsService|ServiceIsRunning|StartService|StopService|RemoveService)\s+"([^"]+)"/g,
))
  assert.equal(
    m[1],
    'clash_verge_dualnic_service',
    'installer references a different service',
  )
for (const m of nsis.matchAll(
  /nsis_tauri_utils::(?:FindProcess(?:CurrentUser)?|KillProcess(?:CurrentUser)?)\s+"([^"]+)"/g,
))
  assert.ok(
    [
      '$' + '{MAINBINARYNAME}.exe',
      'clash-verge-dualnic-service.exe',
      'verge-mihomo-dualnic.exe',
      'verge-mihomo-dualnic-alpha.exe',
    ].includes(m[1]),
    'installer may act on another application process: ' + m[1],
  )
for (const line of nsis
  .split('\n')
  .filter((l) => /^\s*(?:Delete|DeleteRegKey|DeleteRegValue|RMDir)\b/i.test(l)))
  assert.ok(
    !/(?:\\Clash Verge(?: Rev)?(?:\.|\\|"|\b)|\\clash-verge\.lnk|\\ClashVerge")/i.test(
      line,
    ),
    'installer may remove official application state: ' + line,
  )
for (const file of [
  'tauri.conf.json',
  'tauri.windows.conf.json',
  'webview2.x64.json',
]) {
  const config = JSON.parse(read('src-tauri/' + file))
  assert.equal(config.identifier, 'io.github.clash-verge-dualnic')
}
const capability = JSON.parse(read('src-tauri/capabilities/desktop.json'))
assert.ok(
  !capability.permissions.some(
    (p) =>
      typeof p === 'string' &&
      (p.startsWith('updater:') ||
        p === 'mihomo:default' ||
        /^mihomo:allow-(patch-base-config|reload-config|restart|upgrade-core|update-controller|update-secret)$/.test(
          p,
        )),
  ),
)
assert.ok(
  nsis.includes('Refusing to install over the official Clash Verge directory'),
)
console.log(
  'Installer process/service/data isolation, application identifiers and mutation permissions verified.',
)

const serviceEntry = read('src-tauri/src/core/service.rs')
assert.ok(serviceEntry.includes('clash_verge_service_ipc::SERVICE_BINARY_STEM'))
assert.ok(!serviceEntry.includes('format!("clash-verge-service-install'))
assert.ok(
  read('vendor/service-ipc/src/management.rs').includes(
    'crate::SERVICE_BINARY_STEM',
  ),
)
assert.ok(
  !read('vendor/service-ipc/src/management.rs').includes(
    'format!("clash-verge-service{}"',
  ),
)

const tasks = read('src-tauri/src/utils/schtasks.rs')
for (const m of tasks.matchAll(
  /const TASK_NAME_(?:USER|ADMIN): &str = "([^"]+)"/g,
))
  assert.ok(m[1].startsWith('Clash Verge Dual NIC'))
for (const m of tasks.matchAll(/startup_dir\.join\("([^"]+)"\)/g))
  assert.ok(
    ['Clash-Verge-Dual-NIC.lnk', 'Clash Verge Dual NIC.lnk'].includes(m[1]),
  )
const init = read('src-tauri/src/utils/init.rs')
for (const m of init.matchAll(/create_subkey\("(Software[^"]+)"\)/g))
  assert.ok(m[1].startsWith('Software\\\\Classes\\\\clash-verge-dualnic'))
assert.ok(
  read('src-tauri/src/utils/server.rs').includes(
    'arg.starts_with("clash-verge-dualnic:")',
  ),
)

const clientConfig = JSON.parse(read('src-tauri/tauri.conf.json'))
assert.equal(clientConfig.bundle.createUpdaterArtifacts, false)
assert.equal(clientConfig.plugins.updater, undefined)
assert.ok(
  !read('src-tauri/src/lib.rs').includes('tauri_plugin_updater::Builder'),
)
assert.ok(!read('src-tauri/src/core/updater.rs').includes('UpdaterExt'))
assert.ok(
  read('src-tauri/src/cmd/app.rs').includes(
    'This custom client uses manual updates',
  ),
)
for (const capability of [
  'desktop.json',
  'desktop-windows.json',
  'migrated.json',
]) {
  const permissions = JSON.parse(
    read('src-tauri/capabilities/' + capability),
  ).permissions
  assert.ok(
    !permissions.some((p) => typeof p === 'string' && p.startsWith('updater:')),
  )
}
for (const source of [
  'src/components/home/system-info-card.tsx',
  'src/components/setting/mods/misc-viewer.tsx',
  'src/components/layout/layout-sidebar.tsx',
]) {
  assert.ok(!read(source).includes('useUpdate'))
  assert.ok(!read(source).includes('autoCheckUpdate'))
  assert.ok(!read(source).includes('UpdateButton'))
}
console.log('Independent release version and disabled online updater verified.')
