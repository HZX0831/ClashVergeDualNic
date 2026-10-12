import { execFileSync } from 'node:child_process'
import path from 'node:path'
const root = path.resolve(import.meta.dirname, '../..')
for (const [file, args] of [
  ['node_modules/typescript/bin/tsc', ['--noEmit']],
  ['node_modules/vite/bin/vite.js', ['build']],
])
  execFileSync(process.execPath, [path.join(root, file), ...args], {
    cwd: root,
    stdio: 'inherit',
  })
