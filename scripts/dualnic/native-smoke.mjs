import fs from 'node:fs/promises'
import path from 'node:path'
import net from 'node:net'
import dgram from 'node:dgram'
import http from 'node:http'
import { spawn, execFileSync } from 'node:child_process'
import assert from 'node:assert/strict'
import * as yaml from 'js-yaml'
const root = path.resolve(import.meta.dirname, '../..')
const target = process.env.CARGO_TARGET_DIR || path.join(root, 'target')
execFileSync(
  process.env.DUALNIC_CARGO || 'cargo',
  [
    'build',
    '--release',
    '--locked',
    '-p',
    'clash-verge-dualnic',
    '--bin',
    'dualnic-check',
  ],
  { cwd: root, stdio: 'inherit' },
)
const work = await fs.mkdtemp(path.join(target, 'dualnic-native-smoke-'))
for (const name of ['Country.mmdb', 'ASN.mmdb', 'geoip.dat', 'geosite.dat'])
  await fs.copyFile(
    path.join(root, 'src-tauri/resources', name),
    path.join(work, name),
  )
const core =
  process.env.DUALNIC_MIHOMO_SOURCE ||
  path.join(
    root,
    'src-tauri/sidecar/verge-mihomo-dualnic-x86_64-pc-windows-msvc.exe',
  )
const input = {
  proxies: [
    {
      name: 'node',
      type: 'ss',
      server: '203.0.113.2',
      port: 9,
      cipher: 'aes-128-gcm',
      password: 'test',
      udp: false,
    },
  ],
  'proxy-groups': [
    { name: 'Main', type: 'select', proxies: ['DIRECT', 'node'] },
  ],
  rules: ['SUB-RULE,(NETWORK,TCP),one', 'NETWORK,UDP,Main', 'MATCH,PASS'],
  'sub-rules': {
    one: [
      'DOMAIN-SUFFIX,com,Main',
      'DOMAIN,www.baidu.com,REJECT',
      'IP-CIDR,127.0.0.0/8,DIRECT',
      'MATCH,Main',
    ],
  },
  ipv6: false,
}
const adapter = (name) => ({
  guid: name,
  name,
  description: 'synthetic test fixture',
  up: true,
  addresses: ['192.0.2.2'],
  gateways: ['192.0.2.1'],
  ipv4: true,
  ipv6: false,
})
await fs.writeFile(path.join(work, 'input.yaml'), yaml.dump(input))
await fs.writeFile(
  path.join(work, 'context.json'),
  JSON.stringify({
    wired: adapter('以太网'),
    proxy: adapter('DualNIC-Missing-USB'),
    unmatched: adapter('以太网'),
  }),
)
execFileSync(
  path.join(target, 'release/dualnic-check.exe'),
  [
    path.join(work, 'input.yaml'),
    path.join(work, 'context.json'),
    path.join(work, 'candidate.yaml'),
  ],
  { cwd: root, stdio: 'inherit' },
)
execFileSync(
  core,
  ['-t', '-d', work, '-f', path.join(work, 'candidate.yaml')],
  { cwd: root, stdio: 'inherit' },
)
const candidate = yaml.load(
  await fs.readFile(path.join(work, 'candidate.yaml'), 'utf8'),
)
assert.ok(candidate.rules.includes('MATCH,__DualNic_Unmatched'))
assert.ok(candidate.rules.includes('NETWORK,UDP,REJECT'))
const visits = []
const origin = http.createServer((req, res) => {
  visits.push(req.headers.host)
  res.writeHead(200, { 'Content-Type': 'text/plain' })
  res.end('dualnic-lan-ok')
})
await new Promise((r) => origin.listen(0, '127.0.0.1', r))
const port = origin.address().port
const udpOrigin = dgram.createSocket('udp4')
let udpVisits = 0
udpOrigin.on('message', () => udpVisits++)
await new Promise((r) => udpOrigin.bind(0, '127.0.0.1', r))
const udpPort = udpOrigin.address().port
const reservation = net.createServer()
await new Promise((r) => reservation.listen(0, '127.0.0.1', r))
const socksPort = reservation.address().port
await new Promise((r) => reservation.close(r))
const pipe = '\\\\.\\pipe\\dualnic-native-smoke-' + process.pid
candidate.tun.enable = false
candidate['external-controller'] = ''
candidate['external-controller-pipe'] = pipe
candidate['secret'] = ''
candidate['log-level'] = 'debug'
candidate['socks-port'] = socksPort
candidate['bind-address'] = '127.0.0.1'
candidate.hosts = {
  'www.baidu.com': '127.0.0.1',
  'local.example.com': '127.0.0.1',
}
candidate.dns['use-hosts'] = true
await fs.writeFile(path.join(work, 'runtime.yaml'), yaml.dump(candidate))
let output = ''
const child = spawn(core, ['-d', work, '-f', path.join(work, 'runtime.yaml')], {
  cwd: root,
  windowsHide: true,
  stdio: ['ignore', 'pipe', 'pipe'],
})
child.stdout.on('data', (b) => (output += b))
child.stderr.on('data', (b) => (output += b))
const agent = new http.Agent({ keepAlive: false })
agent.createConnection = () => net.createConnection(pipe)
const api = (p) =>
  new Promise((resolve, reject) => {
    const req = http.get({ host: 'localhost', path: p, agent }, (res) => {
      let text = ''
      res.on('data', (b) => (text += b))
      res.on('end', () => {
        try {
          resolve(JSON.parse(text))
        } catch (e) {
          reject(e)
        }
      })
    })
    req.on('error', reject)
    req.setTimeout(1000, () => req.destroy(new Error('API timeout')))
  })
const request = (name) =>
  new Promise((resolve) => {
    const socket = net.createConnection({ host: '127.0.0.1', port: socksPort })
    let phase = 0,
      buffer = Buffer.alloc(0),
      body = '',
      done = false
    const finish = () => {
      if (!done) {
        done = true
        socket.destroy()
        resolve(body)
      }
    }
    socket.setTimeout(4000, finish)
    socket.on('error', (e) => {
      body += 'SOCKS error: ' + e.code
      finish()
    })
    socket.on('close', finish)
    socket.on('connect', () => socket.write(Buffer.from([5, 1, 0])))
    socket.on('data', (chunk) => {
      if (phase === 2) {
        body += chunk.toString()
        return
      }
      buffer = Buffer.concat([buffer, chunk])
      if (phase === 0 && buffer.length >= 2) {
        buffer = buffer.subarray(2)
        phase = 1
        const host = Buffer.from(name)
        const dest = Buffer.alloc(2)
        dest.writeUInt16BE(port)
        socket.write(
          Buffer.concat([Buffer.from([5, 1, 0, 3, host.length]), host, dest]),
        )
      }
      if (phase === 1 && buffer.length >= 5) {
        const size = buffer[3] === 1 ? 10 : buffer[3] === 4 ? 22 : 7 + buffer[4]
        if (buffer.length < size) return
        body += buffer.subarray(size).toString()
        buffer = Buffer.alloc(0)
        phase = 2
        socket.write(
          'GET / HTTP/1.1\r\nHost: ' + name + '\r\nConnection: close\r\n\r\n',
        )
      }
    })
  })
try {
  let version
  for (let i = 0; i < 50; i++) {
    try {
      version = await api('/version')
      break
    } catch {
      if (child.exitCode !== null) throw new Error('Core exited: ' + output)
      await new Promise((r) => setTimeout(r, 100))
    }
  }
  assert.equal(version?.version?.replace(/^v/, ''), '1.19.32')
  await new Promise((r) => setTimeout(r, 300))
  const local = await request('local.example.com')
  if (!local.includes('dualnic-lan-ok'))
    console.log({ local, visits, socksPort, core: output.slice(-1400) })
  assert.ok(
    local.includes('dualnic-lan-ok'),
    'Nested LAN rule must remain reachable after a classification rule',
  )
  const blocked = await request('www.baidu.com')
  assert.ok(
    !blocked.includes('dualnic-lan-ok'),
    'Nested explicit reject must win over domestic routing',
  )
  assert.deepEqual(visits, ['local.example.com'])
  const udp = dgram.createSocket('udp4')
  const udpDestination = Buffer.alloc(2)
  udpDestination.writeUInt16BE(udpPort)
  await new Promise((resolve, reject) =>
    udp.send(
      Buffer.concat([
        Buffer.from([0, 0, 0, 1, 127, 0, 0, 1]),
        udpDestination,
        Buffer.from('dualnic-udp-probe'),
      ]),
      socksPort,
      '127.0.0.1',
      (e) => (e ? reject(e) : resolve()),
    ),
  )
  await new Promise((r) => setTimeout(r, 700))
  udp.close()
  assert.equal(
    udpVisits,
    0,
    'Unsupported UDP must not use the default DIRECT exit',
  )
  assert.ok(
    output
      .split('\n')
      .some((l) => l.includes('[UDP]') && l.includes('using REJECT')),
    'UDP probe must be processed by the kernel and rejected',
  )
  const rules = await api('/rules')
  assert.ok(
    rules.rules.some(
      (r) => r.type === 'Match' && r.proxy === '__DualNic_Unmatched',
    ),
  )
  assert.equal(rules.rules.at(-1).proxy, 'REJECT')
  const live = await api('/configs')
  assert.equal(live.tun.enable, false)
  await fs.writeFile(
    path.join(work, 'result.json'),
    JSON.stringify(
      {
        version: version.version,
        nestedLAN: true,
        nestedReject: true,
        unsupportedUDPRejected: true,
        terminalMatch: '__DualNic_Unmatched',
        tunEnabled: false,
      },
      null,
      2,
    ),
  )
  console.log(
    'Native Mihomo: nested LAN/reject priority, explicit direct default with a matching UDP reject and unsupported UDP rejection verified with owned loopback traffic; TUN disabled.',
  )
} finally {
  agent.destroy()
  const exited =
    child.exitCode !== null || child.signalCode !== null
      ? Promise.resolve()
      : new Promise((resolve) => child.once('exit', resolve))
  child.kill()
  await exited
  udpOrigin.close()
  origin.closeAllConnections()
  await new Promise((r) => origin.close(r))
  if (
    (await fs.stat(path.join(work, 'result.json')).catch(() => null))?.isFile()
  ) {
    if (path.dirname(path.resolve(work)) !== path.resolve(target))
      throw new Error('Unexpected fixture directory')
    await fs.rm(work, { recursive: true, force: true })
  } else {
    await fs.writeFile(path.join(work, 'core.log'), output)
  }
}
