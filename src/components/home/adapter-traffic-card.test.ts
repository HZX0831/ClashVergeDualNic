import { invoke } from '@tauri-apps/api/core'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import type {
  AdapterCounter,
  AdapterTrafficSnapshot,
  DualNicRoute,
} from '@/services/dual-nic'
import { startAdapterTrafficPolling } from '@/services/dual-nic'
import { sampleAdapters } from '@/utils/adapter-traffic-sampler'

import { AdapterTrafficView } from './adapter-traffic-card'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
const adapter: AdapterCounter = {
  guid: 'shared',
  name: '共享网卡',
  description: 'Physical',
  up: true,
  addresses: ['192.0.2.2'],
  gateways: ['192.0.2.1'],
  ipv4: true,
  ipv6: false,
  physical: true,
  interface_id: '1',
  received: '9007199254740993',
  sent: '100',
}
const route: DualNicRoute = {
  primary: adapter,
  effective: adapter,
  state: 'primary',
  reason: 'primaryAvailable',
}
const blocked: DualNicRoute = { state: 'blocked', reason: 'noAvailableAdapter' }
const snapshot: AdapterTrafficSnapshot = {
  sampled_at_ms: 1000,
  adapters: [adapter],
  core_pid: 123,
  status: {
    settings: {
      version: 2,
      enabled: true,
      wired_guid: adapter.guid,
      proxy_guid: adapter.guid,
    },
    state: 'ready',
    routes: { domestic: route, proxy: route, unmatched: blocked },
  },
  sockets: [],
}
const sample = (time: number, patch: Partial<AdapterCounter> = {}) => ({
  ...snapshot,
  sampled_at_ms: time,
  adapters: [{ ...adapter, ...patch }],
})
afterEach(() => {
  vi.useRealTimers()
  vi.clearAllMocks()
})
it('uses exact 64-bit counter differences and actual sample intervals', () => {
  const first = sampleAdapters(new Map(), snapshot)
  const second = sampleAdapters(
    first,
    sample(3000, { received: '9007199254741993', sent: '500' }),
  ).get('shared')!
  expect(second.download).toBe(500)
  expect(second.upload).toBe(200)
  expect(second.receivedTotal).toBe(1000n)
  expect(first.get('shared')!.download).toBe(0)
})
it('resets rate baselines after resets, long gaps, address changes or reconnects', () => {
  const initial = sampleAdapters(new Map(), snapshot)
  for (const update of [
    sample(2000, { received: '1', sent: '1' }),
    sample(10000, { received: '9007199254741993' }),
    sample(2000, { addresses: ['192.0.2.3'] }),
    sample(2000, { interface_id: '2' }),
    sample(2000, { up: false }),
  ]) {
    expect(sampleAdapters(initial, update).get('shared')!.download).toBe(0)
  }
  const down = sampleAdapters(initial, sample(2000, { up: false }))
  expect(
    sampleAdapters(down, sample(3000, { received: '9007199254741993' })).get(
      'shared',
    )!.download,
  ).toBe(0)
  const absent = sampleAdapters(initial, { ...snapshot, adapters: [] })
  expect(absent.size).toBe(0)
  expect(
    sampleAdapters(absent, sample(3000)).get('shared')!.receivedTotal,
  ).toBe(0n)
})
it('bounds trends to the last 60 seconds and deduplicates shared interfaces', () => {
  let samples = sampleAdapters(new Map(), snapshot)
  for (let i = 2; i < 70; i++)
    samples = sampleAdapters(samples, sample(i * 1000))
  expect(samples.get('shared')!.history.length).toBe(61)
  const duplicate = sampleAdapters(samples, {
    ...snapshot,
    adapters: [adapter, adapter],
  })
  expect(duplicate.size).toBe(1)
})
it('shows a shared physical interface once and does not claim proof from counters', () => {
  const html = renderToStaticMarkup(
    createElement(AdapterTrafficView, {
      snapshot,
      samples: sampleAdapters(new Map(), snapshot),
      showVirtual: false,
    }),
  )
  expect(html.match(/IP：/g)?.length).toBe(1)
  expect(html).toContain('国内')
  expect(html).toContain('代理')
  expect(html).toContain('不代表分流失败')
  expect(html).not.toContain('符合配置')
})
it('keeps virtual interfaces optional and displays unavailable socket evidence', () => {
  const virtual = { ...adapter, guid: 'tun', name: '虚拟接口', physical: false }
  const data = {
    ...snapshot,
    adapters: [adapter, virtual],
    socket_error: '采集不可用',
  }
  const props = {
    snapshot: data,
    samples: sampleAdapters(new Map(), data),
    showVirtual: false,
  }
  expect(
    renderToStaticMarkup(createElement(AdapterTrafficView, props)),
  ).not.toContain('虚拟接口')
  const html = renderToStaticMarkup(
    createElement(AdapterTrafficView, { ...props, showVirtual: true }),
  )
  expect(html).toContain('虚拟接口')
  expect(html).toContain('采集不可用')
})
it('shows an egress mismatch separately from a successful configuration', () => {
  const data: AdapterTrafficSnapshot = {
    ...snapshot,
    sockets: [
      {
        local_ip: '192.0.2.3',
        remote_ip: '203.0.113.9',
        remote_port: 443,
        local_port: 12345,
        adapter_guids: [],
        expected_guid: 'shared',
        role: 'proxy',
        status: 'mismatch',
      },
    ],
  }
  const html = renderToStaticMarkup(
    createElement(AdapterTrafficView, {
      snapshot: data,
      samples: sampleAdapters(new Map(), data),
      showVirtual: false,
    }),
  )
  expect(html).toContain('出口不符')
  expect(html).not.toContain('符合配置')
})
it('cancels scheduled polling and discards an in-flight result after hiding', async () => {
  vi.useFakeTimers()
  let resolve!: (value: AdapterTrafficSnapshot) => void
  vi.mocked(invoke).mockImplementation(
    () =>
      new Promise((r) => {
        resolve = r
      }) as never,
  )
  const receive = vi.fn()
  const stop = startAdapterTrafficPolling(receive, vi.fn())
  stop()
  resolve(snapshot)
  await Promise.resolve()
  await vi.advanceTimersByTimeAsync(5000)
  expect(receive).not.toHaveBeenCalled()
  expect(invoke).toHaveBeenCalledTimes(1)
})
it('reports collection failure and continues polling until cancelled', async () => {
  vi.useFakeTimers()
  vi.mocked(invoke)
    .mockRejectedValueOnce(new Error('counter unavailable'))
    .mockResolvedValue(snapshot)
  const receive = vi.fn(),
    fail = vi.fn()
  const stop = startAdapterTrafficPolling(receive, fail)
  await vi.advanceTimersByTimeAsync(1100)
  expect(fail).toHaveBeenCalledTimes(1)
  expect(receive).toHaveBeenCalled()
  stop()
})
