# 实机验证指南

先从托盘完全退出官方版，启动 Clash Verge Dual NIC。根据设备描述选择实际国内和代理网卡，不要选择 Mihomo 的 TUN 虚拟接口。下面的 IP 地址以当前 Windows 网卡地址为准；LAN 目标请选择本机可以访问的内网服务。

## 基本分流与 LAN

1. 应用双网卡设置，分别访问可用的内网、国内公网及订阅代理分类目标。
2. 在首页「网卡流量与出口检测」查看各角色实际网卡与 TCP 源地址。
3. 区分配置核验、连接成功和物理出口证据。UDP 节点没有 TCP 证据时是待抓包状态。
4. 同时检查 USB 所在内网可访问；LAN 通过系统可达路由选择网卡。

## 只读核对 TCP 地址

先访问目标以建立连接，然后在 PowerShell 中运行：

~~~powershell
$dualNicCores = Get-CimInstance Win32_Process | Where-Object { $_.Name -like 'verge-mihomo-dualnic*' }
$dualNicCores | Select-Object Name,ProcessId,ParentProcessId
foreach ($dualNicCore in $dualNicCores) {
  Get-NetTCPConnection -OwningProcess $dualNicCore.ProcessId -ErrorAction SilentlyContinue |
    Where-Object State -eq Established |
    Select-Object LocalAddress,LocalPort,RemoteAddress,RemotePort
}
~~~

国内直连的 LocalAddress 应来自国内有效网卡，订阅节点连接应来自代理有效网卡，未匹配直连应来自已配置的兜底网卡。LAN 地址按可达路由判断。网关或 DHCP 变化后，使用最新网卡地址核对。

## 显式主备与默认兜底

- 三个可选出口留空时应阻断对应无出口流量，不隐式选择其他网卡。主备相同应提示无额外备用能力。
- 配置不同的主备，实际断开主网卡，核对备用源地址和已有连接关闭；恢复主网卡后检查自动切回。
- 两张角色主备均不可用时，对应公网连接应拒绝。网页或节点超时不应触发网卡切换。
- 分别检查默认兜底留空、可用、断线；只有未匹配规则进入默认直连，具体代理及明确拒绝规则保留。
- 所有角色使用同一网卡时，流量只统计一次，不能因为只有一张网卡有流量就判定策略失败。

## DNS、UDP 和界面采样

国内 DNS 随国内有效出口，节点域名 DNS 随代理有效出口。代理不可用且默认兜底可用时，非国内公共 DNS 可经兜底查询，已匹配代理分类的应用流量仍拒绝。实际 DNS/UDP 出口应在物理网卡上抓包核对目的地址、端口和接口；Windows 通配地址 socket 不是出接口证据。IPv6 也需分别核对。

卡片可见时每秒采样，隐藏或离开首页后停止。重连、计数器回退、设备消失或长时间中断后应重新建立基线，避免异常速率。接口累计流量包括所有程序和 LAN。

## 生命周期与订阅

检查订阅切换、更新、新订阅导入、provider 更新和节点切换，分别核对配置及真实源地址。还需测试 DHCP、设备改名、更换 GUID、休眠唤醒、客户端重启与重复应用。

关闭双网卡并应用，应从当前订阅重建普通配置。完全退出定制版后再启动官方版，核对互斥与退出流程。独立服务的管理员安装、停止及卸载需在正常权限环境中验收。

报告问题时提供时间、脱敏后的网卡角色、检测结果和必要日志；不要提交私有订阅链接、节点凭据、控制密钥或原始配置。已完成检查及证据边界见 [验证范围](acceptance.md)。
