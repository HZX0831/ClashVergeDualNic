import { invoke } from '@tauri-apps/api/core'

export interface DualNicSettings {
  version: number
  enabled: boolean
  wired_guid: string
  proxy_guid: string
  restore_tun?: boolean | null
  wired_backup_guid?: string | null
  proxy_backup_guid?: string | null
  unmatched_guid?: string | null
}
export interface PhysicalAdapter {
  guid: string
  name: string
  description: string
  up: boolean
  addresses: string[]
  gateways: string[]
  ipv4: boolean
  ipv6: boolean
}
export interface DualNicRoute {
  primary?: PhysicalAdapter | null
  backup?: PhysicalAdapter | null
  effective?: PhysicalAdapter | null
  state: 'primary' | 'backup' | 'blocked'
  reason: 'primaryAvailable' | 'primaryUnavailable' | 'noAvailableAdapter'
}
export interface DualNicRoutes {
  domestic: DualNicRoute
  proxy: DualNicRoute
  unmatched: DualNicRoute
}
export interface DualNicStatus {
  settings: DualNicSettings
  wired?: PhysicalAdapter | null
  proxy?: PhysicalAdapter | null
  state: string
  routes: DualNicRoutes
  error?: string | null
}
export interface DualNicPreview {
  yaml: string
  validation: ValidationOutcome
  report?: {
    proxy_dns_group: string
    routes: DualNicRoutes
    proxy_count: number
    provider_count: number
    rewritten_direct_rules: string[]
    skipped_nodes: string[]
    wired_ready: boolean
    proxy_ready: boolean
  } | null
}
export const listPhysicalAdapters = () =>
  invoke<PhysicalAdapter[]>('list_physical_adapters')
export const getDualNicStatus = () =>
  invoke<DualNicStatus>('get_dual_nic_status')
export const previewDualNic = (settings: DualNicSettings) =>
  invoke<DualNicPreview>('preview_dual_nic', { settings })
export const applyDualNic = (settings: DualNicSettings) =>
  invoke<DualNicStatus>('apply_dual_nic', { settings })
export interface DualNicDetection {
  configured: boolean
  configuration_error?: string | null
  results: [string, boolean, string][]
  egress: {
    label: string
    interface: string
    source_addresses: string[]
    status: 'verified' | 'mismatch' | 'notObserved' | 'unavailable'
    detail: string
  }[]
}
export const testDualNic = () => invoke<DualNicDetection>('test_dual_nic')
export const importOfficialProfiles = () =>
  invoke<number>('import_official_profiles')

export interface AdapterCounter extends PhysicalAdapter {
  physical: boolean
  interface_id: string
  received: string
  sent: string
}
export interface SocketObservation {
  local_ip: string
  remote_ip: string
  remote_port: number
  local_port: number
  adapter_guids: string[]
  role?: 'domestic' | 'proxy' | 'unmatched' | null
  expected_guid?: string | null
  status: 'verified' | 'mismatch' | 'observed'
}
export interface AdapterTrafficSnapshot {
  sampled_at_ms: number
  adapters: AdapterCounter[]
  status: DualNicStatus
  core_pid?: number | null
  sockets: SocketObservation[]
  socket_error?: string | null
}
export const getAdapterTraffic = () =>
  invoke<AdapterTrafficSnapshot>('get_adapter_traffic')

export const startAdapterTrafficPolling = (
  onSample: (sample: AdapterTrafficSnapshot) => void,
  onError: (error: unknown) => void,
) => {
  let cancelled = false
  let timer: ReturnType<typeof setTimeout> | undefined
  const poll = async () => {
    const started = performance.now()
    try {
      const sample = await getAdapterTraffic()
      if (!cancelled) onSample(sample)
    } catch (error) {
      if (!cancelled) onError(error)
    }
    if (!cancelled)
      timer = setTimeout(
        () => {
          void poll()
        },
        Math.max(0, 1000 - (performance.now() - started)),
      )
  }
  void poll()
  return () => {
    cancelled = true
    clearTimeout(timer)
  }
}
