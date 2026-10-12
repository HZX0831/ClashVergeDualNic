import { Alert, Stack, Typography } from '@mui/material'

import type {
  DualNicRoute,
  DualNicRoutes,
  DualNicSettings,
} from '@/services/dual-nic'

const adapterName = (adapter: DualNicRoute['primary'], guid?: string | null) =>
  adapter?.name || (guid ? '已保存网卡（未接入）' : '未设置')
export const DualNicRoutesView = ({
  routes,
  settings,
}: {
  routes: DualNicRoutes
  settings: DualNicSettings
}) => (
  <Stack spacing={1}>
    {(
      [
        [
          '国内公网',
          routes.domestic,
          settings.wired_guid,
          settings.wired_backup_guid,
        ],
        [
          '代理节点',
          routes.proxy,
          settings.proxy_guid,
          settings.proxy_backup_guid,
        ],
        ['未匹配规则', routes.unmatched, settings.unmatched_guid, null],
      ] as const
    ).map(([label, route, primaryGuid, backupGuid]) => (
      <Alert
        key={label}
        severity={
          !settings.enabled
            ? 'info'
            : route.state === 'blocked'
              ? 'warning'
              : route.state === 'backup'
                ? 'info'
                : 'success'
        }
      >
        {label}：
        {settings.enabled ? route.effective?.name || '拒绝' : '功能未启用'}
        <Typography variant="body2">
          {label === '未匹配规则' ? '直连兜底' : '主出口'}：
          {adapterName(route.primary, primaryGuid)}
          {label !== '未匹配规则' && (
            <>；备用：{adapterName(route.backup, backupGuid)}</>
          )}
        </Typography>
        {settings.enabled && (
          <Typography variant="caption">
            {route.state === 'backup'
              ? '主网卡不可用，正在使用备用；主网卡恢复后自动切回。'
              : route.state === 'blocked'
                ? '没有配置的可用出口，对应流量拒绝。'
                : label === '未匹配规则'
                  ? '使用明确配置的直连兜底网卡。'
                  : '主网卡可用，优先使用主出口。'}
          </Typography>
        )}
      </Alert>
    ))}
  </Stack>
)
