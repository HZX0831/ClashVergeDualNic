# Clash Verge Dual NIC

面向 **Windows x64** 的双网卡分流客户端，基于 [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev) 开发。国内直连、订阅代理和未匹配流量可以分别指定物理网卡，并通过实时流量与 TCP 源地址核对实际出口。

**独立发行版：1.0.0** · **修改日期：2026-10-12** · **许可证：GPL v3**

这是 Clash Verge Rev 的独立修改版。定制版使用独立的应用数据目录和 Windows 服务，版号与上游分别管理；上游官方安装包不包含本项目的双网卡功能。

## 功能

- 为国内直连和订阅代理分别设置主网卡与可选备用网卡。
- 显式配置未匹配规则的直连兜底；留空时拒绝未匹配公网流量。
- 主网卡不可用时切换到已配置的可用备用，恢复后自动切回。
- 预览与运行状态显示主出口、备用出口、当前有效出口及切换原因。
- 首页显示网卡上传/下载速率、观察累计量、60 秒趋势与自有内核 TCP 出口证据。
- 支持只读导入原版订阅和必要偏好，保留订阅分类、明确拒绝规则及 LAN 访问。

## 分流规则

| 流量类型 | 正常出口 | 没有可用出口时 |
| --- | --- | --- |
| 国内域名及中国大陆 IP | 国内主网卡直连，主网卡不可用时使用国内备用 | 拒绝国内公网流量 |
| 已匹配订阅代理分类 | 代理主网卡连接原订阅节点，不可用时使用代理备用 | 拒绝代理流量 |
| 未匹配具体规则 | 显式选择的兜底网卡直连 | 拒绝未匹配公网流量 |
| LAN | 直连，由系统可达路由选择物理接口 | 保持系统路由行为 |

所有出口角色允许使用同一张网卡。主备相同不增加备用能力；没有配置备用时不会自动寻找其他网卡。切换判断依据是网卡连接状态、可用地址与网关，网页或节点超时不会触发切换。

未匹配兜底替代订阅顶层末尾的 MATCH/FINAL；具体分类与拒绝规则继续生效。已匹配的非 LAN DIRECT 规则沿用转代理策略，不能因代理失败自动进入默认直连。详细 DNS 与规则行为见 [使用说明](docs/dualnic/usage.md)。

## 快速开始

1. 从托盘完全退出官方 Clash Verge，再启动定制版。两版可同时安装，活动内核互斥。
2. 运行 clash-verge-dualnic.exe，按需在设置中安装独立 TUN 服务；首次安装需要管理员权限。客户端需要 WebView2。
3. 导入订阅，在 Clash 设置的「双网卡分流」中启用功能，选择国内、代理主网卡及可选备用/兜底。
4. 预览出口配置，点击应用，再通过首页「网卡流量与出口检测」和实际访问核对源地址。

关闭窗口后托盘继续监测；完全退出会停止自有内核。便携版同样使用独立 AppData 目录保存配置。

**旧设置迁移提示：** 设置结构升级到版本 2 时保留主网卡，三个新增可选出口默认留空，未匹配流量会被拒绝。需要默认直连时，请显式选择兜底网卡并应用。

## 如何判断出口正确

网卡计数器包含该网卡上所有程序和 LAN 流量。两张网卡同时有流量，或多个出口使用同一网卡，都不能直接判定分流失败。

TCP 出口证据需要关联目标、端口、自有内核 PID 和物理源地址；证据不足时只展示观测结果。UDP/DNS 仍需要抓包验证。手动出口探测直接检测指定出口，不能替代完整应用规则路径验收。

LAN 由系统可达路由选择网卡，因此 USB 网卡所在的内网也可以访问。更多验证方法见 [实机自测](docs/dualnic/manual-test.md) 和 [验证范围](docs/dualnic/acceptance.md)。

## 固定版本与更新

| 组件 | 固定版本 |
| --- | --- |
| 上游 Clash Verge Rev | 2.5.8 |
| service-ipc | 2.7.6 |
| Mihomo | 1.19.32 |

固定提交和资源 SHA256 见 [dualnic-build-lock.json](dualnic-build-lock.json)。Mihomo 保持固定版本；兼容的 Alpha 文件名也使用同一份稳定内核。

**原版更新检测、静默下载与在线安装已禁用。** 定制版通过手动安装发行包更新。

## 源码与构建

本仓库保存源码、锁文件和构建脚本。便携包、安装包、Geo 数据和构建资源不纳入 Git；如 [Releases](https://github.com/HZX0831/ClashVergeDualNic/releases) 尚无发行资产，需要先准备与构建锁匹配的资源。

构建环境：Windows x64、Node.js 24、pnpm 12、Rust 1.99.0 MSVC、Visual Studio C++ Build Tools、Windows SDK 与 NSIS 3.x。请在 Developer PowerShell 中运行。

1. 克隆仓库，安装锁定依赖：

~~~powershell
git clone https://github.com/HZX0831/ClashVergeDualNic.git
cd ClashVergeDualNic
pnpm install --frozen-lockfile
~~~

2. 将对应的 build-resources ZIP 解压到仓库根目录。脚本会校验 Mihomo、Geo 数据及 NSIS 插件哈希，不会下载最新版内核。
3. 构建客户端与安装包：

~~~powershell
pnpm dualnic:build
pnpm dualnic:test:bundle
~~~

4. 如需打包，将对应的 Mihomo-v1.19.32-source.zip 复制为 third-party/mihomo-v1.19.32-source.zip，保留归档形式，然后运行：

~~~powershell
pnpm dualnic:package
~~~

输出位于 release/<独立版号>/，包含便携包、安装包、客户端/服务源码、固定 Mihomo 源码、构建资源和 SHA256SUMS.txt。完整前置条件、环境变量和资源说明见 [构建指南](docs/dualnic/usage.md#构建) 与 [内核源码说明](third-party/README.md)。

### 相关检查

~~~powershell
pnpm dualnic:test
pnpm dualnic:test:isolation
pnpm dualnic:test:backend
pnpm dualnic:test:native
pnpm typecheck
pnpm test -- src/components/home/adapter-traffic-card.test.ts src/components/setting/mods/dual-nic-detection-view.test.ts
~~~

独立服务完整测试：pnpm dualnic:test:service。验证记录包含权限环境限制与尚未完成的硬件场景。上游工作流保留供开发参考；新仓库的 GitHub Actions 暂时禁用，定制版自动构建与发布流程需适配后再启用。

## 仓库结构

| 路径 | 内容 |
| --- | --- |
| src/ | React 界面与前端服务 |
| src-tauri/ | Tauri 客户端、配置事务和内核生命周期 |
| crates/clash-verge-dualnic/ | 分流策略、Windows 网卡与 TCP 采集 |
| vendor/service-ipc/ | 独立服务源码及原始许可证 |
| scripts/dualnic/ | 定制构建、验证与打包脚本 |
| docs/dualnic/ | 使用、验证和许可证说明 |
| third-party/ | Mihomo 许可证与源码准备说明 |

其他 crates、脚本及多语言说明来自上游；定制行为与支持平台以本 README 为准。

## 已知限制

- 当前定制功能及发行目标为 Windows x64。
- 不支持代理链、provider override-expr、额外 TUN listener 或自定义 tunnels，冲突配置会阻止应用。
- 出口保护适用于自有内核与 TUN 正在运行期间的接管流量；订阅下载保持上游行为。
- 实际主备拔插/恢复、UDP/DNS 出口、IPv6 和完整管理员服务场景仍需按具体环境核验，详见验证范围。

## 许可证与致谢

本项目保留原作者版权，继续使用 [GPL v3](LICENSE)。允许修改、使用和再分发；分发修改版时须保留版权与许可证、标明修改和日期，并提供对应源码及构建/安装脚本。主要修改包括双网卡出口策略、显式备用与兜底、网卡流量和 TCP 出口检测、独立服务身份及订阅迁移/恢复。

Mihomo、service-ipc 和 SimpleSC 保留各自授权与来源，详见 [许可证检查](docs/dualnic/license-review.md)。隐私说明见 [PRIVACY.md](PRIVACY.md)。

感谢 [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev)、[zzzgydi/Clash Verge](https://github.com/zzzgydi/clash-verge)、[Mihomo](https://github.com/MetaCubeX/mihomo)、[Tauri](https://github.com/tauri-apps/tauri)、[Clash](https://github.com/Dreamacro/clash)、[Clash for Windows](https://github.com/Fndroid/clash_for_windows_pkg) 和 [Vite](https://github.com/vitejs/vite)。
