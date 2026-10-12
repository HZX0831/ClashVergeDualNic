import {
  Alert,
  Box,
  Button,
  Dialog,
  DialogContent,
  DialogTitle,
  FormControl,
  InputLabel,
  MenuItem,
  Select,
  Stack,
  Switch,
  Typography,
} from '@mui/material'
import { useState } from 'react'
import useSWR from 'swr'

import {
  applyDualNic,
  getDualNicStatus,
  importOfficialProfiles,
  listPhysicalAdapters,
  previewDualNic,
  testDualNic,
  type DualNicSettings,
  type DualNicDetection,
  type DualNicRoutes,
} from '@/services/dual-nic'
import { mutate } from '@/services/mutate'
import { showNotice } from '@/services/notice-service'

import { DualNicDetectionView } from './dual-nic-detection-view'
import { DualNicRoutesView } from './dual-nic-routes-view'

const initial: DualNicSettings = {
  version: 2,
  enabled: false,
  wired_guid: '',
  proxy_guid: '',
}
const states: Record<string, string> = {
  disabled: '未启用',
  ready: '国内与代理出口可用',
  proxyOffline: '代理出口不可用，代理流量拒绝',
  wiredOffline: '国内出口不可用，国内公网拒绝',
  offline: '国内与代理出口不可用',
  unmatchedOnly: '仅默认直连兜底可用',
}
export const DualNicSettingsCard = () => {
  const [detection, setDetection] = useState<DualNicDetection | null>(null)
  const { data: status, mutate: refreshStatus } = useSWR(
    ['getDualNicStatus'],
    getDualNicStatus,
    { refreshInterval: 30000 },
  )
  const { data: adapters = [], mutate: refreshAdapters } = useSWR(
    ['listPhysicalAdapters'],
    listPhysicalAdapters,
  )
  const [settings, setSettings] = useState<DualNicSettings | null>(null)
  const [busy, setBusy] = useState(false)
  const [previewRoutes, setPreviewRoutes] = useState<DualNicRoutes | null>(null)
  const [details, setDetails] = useState<string | null>(null)
  const selected = settings || {
    ...initial,
    ...status?.settings,
    wired_guid:
      status?.settings.wired_guid ||
      adapters.find((a) => a.name === '以太网')?.guid ||
      '',
  }
  const action = async (work: () => Promise<void>) => {
    setBusy(true)
    try {
      await work()
    } catch (e) {
      showNotice.error(e)
    } finally {
      setBusy(false)
    }
  }
  const selector = (
    field:
      | 'wired_guid'
      | 'proxy_guid'
      | 'wired_backup_guid'
      | 'proxy_backup_guid'
      | 'unmatched_guid',
    label: string,
  ) => (
    <FormControl fullWidth size="small">
      <InputLabel>{label}</InputLabel>
      <Select
        label={label}
        value={selected[field] || ''}
        disabled={busy}
        onChange={(e) => setSettings({ ...selected, [field]: e.target.value })}
      >
        <MenuItem value="">
          {field === 'wired_guid' || field === 'proxy_guid'
            ? '请选择网卡'
            : '不设置（不兜底）'}
        </MenuItem>
        {selected[field] &&
          !adapters.some((a) => a.guid === selected[field]) && (
            <MenuItem value={selected[field]}>已保存网卡当前未接入</MenuItem>
          )}
        {adapters.map((a) => (
          <MenuItem key={a.guid} value={a.guid}>
            <Box>
              <Typography variant="body2">
                {a.name} · {a.up ? '已连接' : '未连接'}
              </Typography>
              <Typography variant="caption">
                {a.description} · IP {a.addresses.join(', ') || '无'} · 网关{' '}
                {a.gateways.join(', ') || '无'}
              </Typography>
            </Box>
          </MenuItem>
        ))}
      </Select>
    </FormControl>
  )
  return (
    <Box sx={{ p: 2, borderTop: 1, borderColor: 'divider' }}>
      <Stack spacing={1.5}>
        <Stack
          direction="row"
          sx={{ justifyContent: 'space-between', alignItems: 'center' }}
        >
          <Typography variant="subtitle1">双网卡分流</Typography>
          <Switch
            checked={selected.enabled}
            disabled={busy}
            onChange={(_, enabled) => setSettings({ ...selected, enabled })}
          />
        </Stack>
        <Typography variant="body2">
          {status ? states[status.state] || status.state : '正在读取网卡…'}
        </Typography>
        {status?.error && <Alert severity="error">{status.error}</Alert>}
        {selector('wired_guid', '国内出口网卡')}
        {selector('wired_backup_guid', '国内备用网卡（可选）')}
        {selector('proxy_guid', '代理出口网卡')}
        {selector('proxy_backup_guid', '代理备用网卡（可选）')}
        {selector('unmatched_guid', '未匹配规则的直连兜底网卡（可选）')}
        {((selected.wired_backup_guid &&
          selected.wired_guid.toLowerCase() ===
            selected.wired_backup_guid.toLowerCase()) ||
          (selected.proxy_backup_guid &&
            selected.proxy_guid.toLowerCase() ===
              selected.proxy_backup_guid.toLowerCase())) && (
          <Alert severity="info">主备选择了同一网卡，无额外备用能力。</Alert>
        )}
        <Alert severity="info">
          未匹配规则的流量：不设置直连兜底就拒绝；订阅末尾 MATCH/FINAL
          不再决定默认出口。已匹配的分类规则继续生效。
        </Alert>
        {status && (
          <DualNicRoutesView
            routes={status.routes}
            settings={status.settings}
          />
        )}
        <Typography variant="caption">
          启用后管理规则模式、TUN 和 DNS
          出口。网卡断开或失去地址/网关时切换，网页超时不触发切换。代理出口不可用时，已设置的默认直连兜底也用于非国内公共
          DNS。关闭窗口继续托盘监测。
        </Typography>
        <Stack direction="row" spacing={1} sx={{ flexWrap: 'wrap' }}>
          <Button
            disabled={busy}
            onClick={() =>
              void action(async () => {
                await refreshAdapters()
                await refreshStatus()
              })
            }
          >
            刷新
          </Button>
          <Button
            disabled={busy}
            onClick={() =>
              void action(async () => {
                const p = await previewDualNic({ ...selected, version: 2 })
                setPreviewRoutes(p.report?.routes || null)
                setDetails(
                  JSON.stringify(
                    { validation: p.validation, report: p.report },
                    null,
                    2,
                  ) +
                    '\n\n' +
                    p.yaml,
                )
              })
            }
          >
            预览
          </Button>
          <Button
            variant="contained"
            disabled={busy}
            onClick={() =>
              void action(async () => {
                const r = await mutate(
                  () => applyDualNic({ ...selected, version: 2 }),
                  {
                    id: 'dual-nic-policy',
                    errorNotice: false,
                    revalidate: [
                      ['getVergeConfig'],
                      ['getClashMode'],
                      ['getClashConfig'],
                    ],
                  },
                )
                if (r.ok) {
                  setSettings(r.value.settings)
                  await refreshStatus(r.value)
                }
              })
            }
          >
            应用
          </Button>
          <Button
            disabled={busy || !status?.settings.enabled}
            onClick={() =>
              void action(async () => {
                setPreviewRoutes(null)
                setDetection(await testDualNic())
              })
            }
          >
            连接检测
          </Button>
          <Button
            disabled={busy || !status?.settings.enabled}
            onClick={() =>
              void action(async () => {
                const r = await mutate(
                  () => applyDualNic({ ...status!.settings, enabled: false }),
                  {
                    id: 'dual-nic-policy',
                    errorNotice: false,
                    revalidate: [
                      ['getVergeConfig'],
                      ['getClashMode'],
                      ['getClashConfig'],
                    ],
                  },
                )
                if (r.ok) {
                  setSettings(r.value.settings)
                  await refreshStatus(r.value)
                }
              })
            }
          >
            关闭并恢复
          </Button>
          <Button
            disabled={busy || status?.settings.enabled}
            onClick={() =>
              void action(async () => {
                setPreviewRoutes(null)
                const r = await mutate(importOfficialProfiles, {
                  id: 'profiles-import',
                  errorNotice: false,
                  revalidate: [['getProfiles'], ['getVergeConfig']],
                })
                if (r.ok)
                  setDetails(
                    '已导入 ' +
                      r.value +
                      ' 个订阅。原版数据保持不变，请退出原版后重启定制客户端。',
                  )
              })
            }
          >
            导入原版订阅
          </Button>
        </Stack>
        <Typography variant="caption">
          官方版与定制版不能同时运行 TUN。连接检测会采集 TCP 源地址；UDP
          节点仍需抓包核对。
        </Typography>
      </Stack>
      <Dialog
        open={details !== null || detection !== null}
        onClose={() => {
          setDetails(null)
          setDetection(null)
          setPreviewRoutes(null)
        }}
        maxWidth="md"
        fullWidth
      >
        <DialogTitle>双网卡预览 / 检测结果</DialogTitle>
        <DialogContent>
          {previewRoutes && (
            <DualNicRoutesView routes={previewRoutes} settings={selected} />
          )}
          {detection ? (
            <DualNicDetectionView result={detection} />
          ) : (
            <Box
              component="pre"
              sx={{
                whiteSpace: 'pre-wrap',
                overflowWrap: 'anywhere',
                fontSize: 12,
              }}
            >
              {details}
            </Box>
          )}
        </DialogContent>
      </Dialog>
    </Box>
  )
}
