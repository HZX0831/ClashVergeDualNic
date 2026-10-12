import fs from 'node:fs'
import path from 'node:path'

export const root = path.resolve(import.meta.dirname, '../..')
export const version = JSON.parse(
  fs.readFileSync(path.join(root, 'package.json'), 'utf8'),
).version
const tauri = JSON.parse(
  fs.readFileSync(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8'),
)
const cargo = fs.readFileSync(path.join(root, 'src-tauri/Cargo.toml'), 'utf8')
const lock = fs.readFileSync(path.join(root, 'Cargo.lock'), 'utf8')
if (
  !/^\d+\.\d+\.\d+$/.test(version) ||
  tauri.version !== version ||
  cargo.match(/^version = "([^"]+)"/m)?.[1] !== version ||
  !lock.includes(
    'name = "clash-verge-dualnic-app"\nversion = "' + version + '"',
  )
)
  throw new Error(
    'Custom client versions must agree in package.json, Cargo.toml, Cargo.lock and tauri.conf.json',
  )
