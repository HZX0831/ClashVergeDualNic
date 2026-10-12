import { Alert, Stack, Typography } from '@mui/material'

import type { DualNicDetection } from '@/services/dual-nic'

export const DualNicDetectionView = ({
  result,
}: {
  result: DualNicDetection
}) => (
  <Stack spacing={1.5}>
    <Alert severity={result.configured ? 'success' : 'error'}>
      {result.configured ? '运行配置符合双网卡策略' : '运行配置校验失败'}
      {result.configuration_error && (
        <Typography variant="body2">{result.configuration_error}</Typography>
      )}
    </Alert>
    <Typography variant="subtitle2">连接与 DNS</Typography>
    {result.results.map(([label, success, detail]) => (
      <Alert key={label} severity={success ? 'success' : 'error'}>
        {label}：{success ? '成功' : '失败'}
        <Typography variant="body2">{detail}</Typography>
      </Alert>
    ))}
    <Typography variant="subtitle2">TCP 出口实测</Typography>
    {result.egress.map((evidence) => (
      <Alert
        key={evidence.label}
        severity={
          evidence.status === 'verified'
            ? 'success'
            : evidence.status === 'mismatch'
              ? 'error'
              : 'warning'
        }
      >
        {evidence.label}：
        {evidence.status === 'verified'
          ? '已核对物理源地址'
          : evidence.status === 'mismatch'
            ? '出口与配置不符'
            : '尚未取得出口证据'}
        <Typography variant="body2">
          网卡：{evidence.interface || '未获取'}；源地址：
          {evidence.source_addresses.join(', ') || '未采集'}
        </Typography>
        <Typography variant="body2">{evidence.detail}</Typography>
      </Alert>
    ))}
    <Typography variant="caption">
      连接成功、配置正确和 TCP
      出口证据分别显示。出口探测直接测试指定出口，不能代替应用规则路径验收。UDP/DNS
      实际出口待抓包验证。
    </Typography>
  </Stack>
)
