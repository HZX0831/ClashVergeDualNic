import type {
  AdapterCounter,
  AdapterTrafficSnapshot,
} from '@/services/dual-nic'

export interface AdapterSample {
  adapter: AdapterCounter
  time: number
  received: bigint
  sent: bigint
  download: number
  upload: number
  receivedTotal: bigint
  sentTotal: bigint
  history: { time: number; download: number; upload: number }[]
}
export const sampleAdapters = (
  previous: Map<string, AdapterSample>,
  snapshot: Pick<AdapterTrafficSnapshot, 'sampled_at_ms' | 'adapters'>,
) => {
  const next = new Map<string, AdapterSample>()
  for (const adapter of snapshot.adapters) {
    const old = previous.get(adapter.guid)
    const received = BigInt(adapter.received)
    const sent = BigInt(adapter.sent)
    const time = snapshot.sampled_at_ms
    const elapsed = old ? time - old.time : 0
    const valid =
      old &&
      elapsed > 0 &&
      elapsed <= 5000 &&
      old.adapter.up &&
      adapter.up &&
      old.adapter.interface_id === adapter.interface_id &&
      old.adapter.addresses.join(',') === adapter.addresses.join(',') &&
      received >= old.received &&
      sent >= old.sent
    const receivedDelta = valid ? received - old.received : 0n
    const sentDelta = valid ? sent - old.sent : 0n
    const download = valid ? (Number(receivedDelta) * 1000) / elapsed : 0
    const upload = valid ? (Number(sentDelta) * 1000) / elapsed : 0
    next.set(adapter.guid, {
      adapter,
      time,
      received,
      sent,
      download,
      upload,
      receivedTotal: (old?.receivedTotal || 0n) + receivedDelta,
      sentTotal: (old?.sentTotal || 0n) + sentDelta,
      history: [
        ...(valid ? old.history : []),
        { time, download, upload },
      ].filter((p) => p.time >= time - 60000),
    })
  }
  return next
}
