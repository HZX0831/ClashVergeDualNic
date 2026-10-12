import { RouterOutlined } from '@mui/icons-material'
import {
  Alert,
  Box,
  Button,
  Chip,
  FormControlLabel,
  Stack,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  Typography,
  useTheme,
} from '@mui/material'
import { useEffect, useState } from 'react'

import { DualNicDetectionView } from '@/components/setting/mods/dual-nic-detection-view'
import { DualNicRoutesView } from '@/components/setting/mods/dual-nic-routes-view'
import { useVisibility } from '@/hooks/use-visibility'
import {
  startAdapterTrafficPolling,
  testDualNic,
  type AdapterTrafficSnapshot,
  type DualNicDetection,
} from '@/services/dual-nic'
import {
  sampleAdapters,
  type AdapterSample,
} from '@/utils/adapter-traffic-sampler'
import parseTraffic from '@/utils/parse-traffic'

import { EnhancedCard } from './enhanced-card'

const amount = (bytes: number | bigint) => parseTraffic(Number(bytes)).join(' ')
const endpoint = (ip: string, port: number) =>
  (ip.includes(':') ? '[' + ip + ']' : ip) + ':' + port
const roles = { domestic: '国内', proxy: '代理', unmatched: '默认直连' }
const Trend = ({ sample }: { sample: AdapterSample }) => {
  const theme = useTheme()
  const scale = Math.max(
    1,
    ...sample.history.flatMap((p) => [p.upload, p.download]),
  )
  const points = (key: 'upload' | 'download') =>
    sample.history
      .map(
        (p) =>
          (p.time - sample.time + 60000) / 200 +
          ',' +
          (48 - (p[key] / scale) * 44),
      )
      .join(' ')
  return (
    <Box sx={{ color: 'text.secondary' }}>
      <svg
        viewBox="0 0 300 52"
        width="100%"
        height="60"
        role="img"
        aria-label={sample.adapter.name + '最近60秒上传与下载趋势'}
      >
        <path d="M0 48H300" stroke={theme.palette.divider} />
        <polyline
          points={points('upload')}
          fill="none"
          stroke={theme.palette.primary.main}
          strokeWidth="2"
        />
        <polyline
          points={points('download')}
          fill="none"
          stroke={theme.palette.success.main}
          strokeWidth="2"
        />
      </svg>
      <Typography variant="caption">
        最近 60 秒 · 蓝色上传 / 绿色下载
      </Typography>
    </Box>
  )
}
export const AdapterTrafficView = ({
  snapshot,
  samples,
  showVirtual,
}: {
  snapshot: AdapterTrafficSnapshot
  samples: Map<string, AdapterSample>
  showVirtual: boolean
}) => (
  <Stack spacing={2}>
    {snapshot.status.settings.enabled && (
      <DualNicRoutesView
        routes={snapshot.status.routes}
        settings={snapshot.status.settings}
      />
    )}
    {snapshot.status.error && (
      <Alert severity="error">{snapshot.status.error}</Alert>
    )}
    {[...samples.values()]
      .filter((s) => showVirtual || s.adapter.physical)
      .map((sample) => {
        const assigned = snapshot.status.settings.enabled
          ? Object.entries(snapshot.status.routes)
              .filter(
                ([, route]) => route.effective?.guid === sample.adapter.guid,
              )
              .map(([role]) => roles[role as keyof typeof roles])
          : []
        return (
          <Box
            key={sample.adapter.guid}
            sx={{ border: 1, borderColor: 'divider', borderRadius: 1, p: 1.5 }}
          >
            <Stack
              direction="row"
              spacing={1}
              sx={{ alignItems: 'center', flexWrap: 'wrap' }}
            >
              <Typography variant="subtitle2">{sample.adapter.name}</Typography>
              <Chip
                size="small"
                label={sample.adapter.up ? '已连接' : '未连接'}
              />
              {!sample.adapter.physical && (
                <Chip size="small" label="虚拟 / 其他接口" />
              )}
              {assigned.map((role) => (
                <Chip
                  key={role}
                  size="small"
                  color="primary"
                  variant="outlined"
                  label={role}
                />
              ))}
            </Stack>
            <Typography variant="caption" sx={{ overflowWrap: 'anywhere' }}>
              {sample.adapter.description} · IP：
              {sample.adapter.addresses.join(', ') || '无'}
            </Typography>
            <Typography variant="body2">
              上传 {amount(sample.upload)}/s · 下载 {amount(sample.download)}/s
            </Typography>
            <Typography variant="caption">
              本次观察：上传 {amount(sample.sentTotal)} · 下载{' '}
              {amount(sample.receivedTotal)}
            </Typography>
            <Trend sample={sample} />
          </Box>
        )
      })}
    <Typography variant="subtitle2">
      当前自有内核 TCP 连接
      {snapshot.core_pid ? ' · PID ' + snapshot.core_pid : ''}
    </Typography>
    {snapshot.socket_error && (
      <Alert severity="info">{snapshot.socket_error}</Alert>
    )}
    <Box sx={{ maxHeight: 260, overflow: 'auto' }}>
      <Table size="small" stickyHeader>
        <TableHead>
          <TableRow>
            <TableCell>物理源地址</TableCell>
            <TableCell>远端地址</TableCell>
            <TableCell>实际网卡</TableCell>
            <TableCell>判断</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {snapshot.sockets.map((socket) => (
            <TableRow
              key={
                socket.local_ip +
                socket.remote_ip +
                socket.remote_port +
                ':' +
                socket.local_port
              }
            >
              <TableCell sx={{ overflowWrap: 'anywhere' }}>
                {endpoint(socket.local_ip, socket.local_port)}
              </TableCell>
              <TableCell sx={{ overflowWrap: 'anywhere' }}>
                {endpoint(socket.remote_ip, socket.remote_port)}
              </TableCell>
              <TableCell>
                {socket.adapter_guids
                  .map(
                    (guid) =>
                      snapshot.adapters.find((a) => a.guid === guid)?.name ||
                      guid,
                  )
                  .join(', ') || '未关联物理网卡'}
              </TableCell>
              <TableCell>
                <Typography
                  variant="caption"
                  color={
                    socket.status === 'mismatch'
                      ? 'error'
                      : socket.status === 'verified'
                        ? 'success.main'
                        : 'text.secondary'
                  }
                >
                  {socket.role && roles[socket.role] + '：'}
                  {socket.status === 'verified'
                    ? '符合配置'
                    : socket.status === 'mismatch'
                      ? '出口不符'
                      : '仅观测'}
                </Typography>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </Box>
    {snapshot.sockets.length === 0 && (
      <Typography variant="body2">
        当前没有采集到自有内核 TCP 连接；不代表分流失败。
      </Typography>
    )}
  </Stack>
)
export const AdapterTrafficCard = () => {
  const visible = useVisibility()
  const [snapshot, setSnapshot] = useState<AdapterTrafficSnapshot | null>(null)
  const [samples, setSamples] = useState(() => new Map<string, AdapterSample>())
  const [error, setError] = useState<string | null>(null)
  const [showVirtual, setShowVirtual] = useState(false)
  const [busy, setBusy] = useState(false)
  const [detectionError, setDetectionError] = useState<string | null>(null)
  const [detection, setDetection] = useState<{
    signature: string
    result: DualNicDetection
  } | null>(null)
  const signature = snapshot
    ? JSON.stringify([
        snapshot.core_pid,
        snapshot.status.settings,
        snapshot.status.routes,
      ])
    : ''
  useEffect(() => {
    if (!visible) return
    return startAdapterTrafficPolling(
      (data) => {
        setSnapshot(data)
        setSamples((previous) => sampleAdapters(previous, data))
        setError(null)
      },
      (e) => setError(String(e)),
    )
  }, [visible])
  const detect = async () => {
    setBusy(true)
    setDetectionError(null)
    try {
      setDetection({ signature, result: await testDualNic() })
    } catch (e) {
      setDetectionError(String(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <EnhancedCard title="网卡流量与出口检测" icon={<RouterOutlined />}>
      <Stack spacing={2}>
        <Alert severity="info">
          网卡计数包含本机所有程序及 LAN 流量，不能单凭流量增长证明分流。TCP
          结合实际源地址判断；UDP/DNS 实际出口待抓包验证。
        </Alert>
        <Stack
          direction="row"
          spacing={1}
          sx={{ alignItems: 'center', flexWrap: 'wrap' }}
        >
          <FormControlLabel
            control={
              <Switch
                size="small"
                checked={showVirtual}
                onChange={(_, value) => setShowVirtual(value)}
              />
            }
            label="显示虚拟网卡"
          />
          <Button
            size="small"
            variant="outlined"
            disabled={
              busy || !snapshot?.status.settings.enabled || !snapshot.core_pid
            }
            onClick={() => {
              void detect()
            }}
          >
            {busy ? '检测中…' : '出口检测'}
          </Button>
        </Stack>
        {error && (
          <Alert severity="error">
            采集或检测失败：{error}。显示的数据可能已过期。
          </Alert>
        )}
        {snapshot ? (
          <AdapterTrafficView
            snapshot={snapshot}
            samples={samples}
            showVirtual={showVirtual}
          />
        ) : (
          !error && <Typography variant="body2">正在读取网卡计数器…</Typography>
        )}
        {detectionError && (
          <Alert severity="error">出口检测失败：{detectionError}</Alert>
        )}
        {detection?.signature === signature && (
          <DualNicDetectionView result={detection.result} />
        )}
      </Stack>
    </EnhancedCard>
  )
}
