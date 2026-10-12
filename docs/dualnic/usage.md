# 使用与构建说明

Windows x64 双物理网卡定制客户端，基于 Clash Verge Rev 2.5.8、service-ipc 2.7.6 和固定 Mihomo 1.19.32。分流与转发由 Mihomo 完成；客户端负责网卡选择、最终配置策略、运行校验和后台监测。

本说明适用于定制版 1.0.0。固定上游提交与资源 SHA256 见根目录 [构建锁](../../dualnic-build-lock.json)。

## 使用

1. 退出官方 Clash Verge 及其活动内核。两版可同时安装，运行时互斥，不会结束官方进程。
2. 解压便携 ZIP 或安装定制安装包，运行 clash-verge-dualnic.exe。便携包无需安装客户端，但需要 WebView2；独立 TUN 服务首次安装需要管理员权限。
3. 在设置中安装独立服务，只安装 clash_verge_dualnic_service。
4. 可在双网卡卡片中一次性复制原版订阅、扩展与必要偏好。原版目录只读；定制版不能已有订阅。导入后重启客户端。旧配置助手代码只从复制的扩展脚本移除。
5. 在 Clash 设置的“双网卡分流”卡片选择国内和代理主网卡，并按需选择国内备用、代理备用、未匹配规则的直连兜底网卡，预览后应用。所有角色允许选择相同网卡；备用和兜底留空时不会自动选择出口。
6. 运行连接检测并核对抓包或连接本地地址。关闭主窗口后托盘继续监测；完全退出会停止本客户端内核。
7. 关闭双网卡开关并点击应用，会按当前订阅生成普通配置，并恢复启用前的 TUN 开关。

国内域名或中国大陆 IP 经国内有效网卡直连，已匹配订阅代理分类的公网 TCP/UDP 经代理有效网卡到订阅节点。未匹配具体规则的流量仅在设置了可用默认兜底网卡时直连，否则拒绝；订阅顶层 MATCH/FINAL 默认规则由该设置替代。保留 LAN 与明确拒绝规则；分类组及节点仍由 Verge 选择。末尾拒绝规则防止不支持 UDP 的节点落入内核默认直连。GUID 是设备身份，名称用于内核绑定。网卡恢复后重新生成当前订阅配置，普通节点切换不额外触发监测重载。

主网卡不可用时使用明确配置的可用备用网卡，恢复后自动切回；没有可用代理主备时移除代理节点与 provider，代理分类连接拒绝，没有可用国内主备时国内公网拒绝。默认直连兜底独立判断。网卡变化的重载或关闭连接失败会停止自有内核并报告原因。断线保护以自有内核和 TUN 正在运行为前提；完全退出或两版切换后不提供系统防火墙保护。验收范围为 TUN 接管的公网 TCP/UDP，订阅下载保持上游行为。

## 服务与版本切换

客户端和安装准备程序使用独立服务文件名。安装全程使用 clash-verge-dualnic-service、clash-verge-dualnic-service-install 和 clash-verge-dualnic-service-uninstall；缺少安装程序时显示实际缺失路径。

原版运行时，本版会提示内核占用并暂停内核。这是互斥保护；更换到本版前，应从原版托盘菜单完全退出，再在本版点击重启内核。反向切换时先完全退出本版。关闭窗口仍会保留托盘后台。安装服务失败与该保护是两个独立问题，无需卸载原版服务。

实际服务安装 EXE 的 --check-only 模式用于验证安装资源准备，覆盖中文路径和文件名隔离；该模式不申请管理员权限、不注册服务，也不证明实际管理员安装成功。完成构建后可运行 node scripts/dualnic/install-bundle-test.mjs 重复此检查。

## 隔离身份

| 项目 | 定制值 |
| --- | --- |
| 应用 / 数据目录 | io.github.clash-verge-dualnic / %APPDATA%\io.github.clash-verge-dualnic |
| Windows 服务 | clash_verge_dualnic_service |
| 服务管道 | \\.\pipe\clash-verge-dualnic-service |
| 服务内核管道 | \\.\pipe\verge-mihomo-dual-nic-{ownerHash} |
| Sidecar 管道 | \\.\pipe\verge-mihomo-dualnic-sidecar-{flavor}-{ownerHash} |
| 服务数据 | %ProgramData%\clash-verge-service-dualnic |
| 深链接 | clash-verge-dualnic:// |
| 开机计划任务 | Clash Verge Dual NIC / Clash Verge Dual NIC (Admin) |

服务认证、会话、ACL、内核来源与 SHA256 校验保持上游机制。核心执行互斥锁继续与上游协作，防止并行 TUN；服务文件、会话与用户数据隔离。便携版使用独立 AppData 目录保存设置。

## 构建

需要 Windows x64、Node.js 24、pnpm 12、Rust 1.99.0 的 x86_64-pc-windows-msvc 工具链、Visual Studio C++ Build Tools 与 Windows SDK、NSIS 3.x、WebView2。在 Developer PowerShell 中构建，使 MSVC 与 SDK 的 INCLUDE/LIB/PATH 生效。Rust 版本由 rust-toolchain.toml 固定。

解压源码和配套 build-resources ZIP 到同一源码根目录，然后依次执行：

    pnpm install --frozen-lockfile
    pnpm dualnic:test
    pnpm dualnic:test:isolation
    pnpm dualnic:test:service
    pnpm dualnic:test:backend
    pnpm dualnic:test:native
    pnpm dualnic:build

后端测试包装器用 Windows SDK 的 mt.exe 为 Rust 测试程序加入 Common Controls v6 清单；客户端 EXE 已由 Tauri 加入该清单。可用 DUALNIC_MT 指定 SDK 的 mt.exe。

构建先校验固定内核、Geo 数据与 NSIS 插件哈希，构建独立服务，再构建前端、客户端和 NSIS 安装包。不会下载最新版或 Alpha Mihomo。兼容上游的 Alpha 文件名指向同一份固定稳定内核，运行时只允许稳定内核选项。官方在线更新入口已禁用，定制包手动更新。构建完成后执行 pnpm dualnic:package，生成便携 ZIP、源码 ZIP、构建资源 ZIP、安装程序和 SHA256SUMS.txt。

资源在其他目录时可设置 DUALNIC_MIHOMO_SOURCE、DUALNIC_ASSETS_SOURCE、DUALNIC_NSIS_PLUGIN_DIR。DUALNIC_CARGO 可指定 cargo.exe；RUSTC/RUSTDOC/CARGO_HOME/RUSTUP_HOME 使用正常 Rust 设置。Cargo.lock 与 pnpm-lock.yaml 固定依赖，构建禁止无锁更新。构建脚本不会安装服务或修改原版数据。

主要源码：

- crates/clash-verge-dualnic：策略、Windows API、回归测试。
- src-tauri/src/core/dual_nic.rs：预览、应用、监测、核验。
- src-tauri/src/core/dual_nic_import.rs：只读迁移。
- src/components/setting/mods/dual-nic-settings.tsx：设置卡片。
- vendor/service-ipc：独立服务完整源码。

## 验证与限制

分流基本功能与独立服务安装已有实机使用反馈。自动测试范围、证据边界及尚未完成的主备拔插、UDP/DNS、IPv6 和管理员服务专项见 [验证范围](acceptance.md)。

不支持代理链、provider override-expr、额外 TUN listener 和自定义 tunnels，冲突会阻止应用。无法证明仅匹配 LAN 的直连规则按公网处理，并列入预览。用于代理与 DNS 选择的内部组名称含 DNS 分隔符 & 或 = 时要求先改名，避免选择器含义不明确。DNS 路径由双网卡功能覆盖；hosts、fake-IP 过滤和可识别 LAN DNS 偏好保留。原先关闭 IPv6 的设置保持关闭；两张网卡有无 IPv6 地址与网关分别判断，实际连通性仍需检测。

上游服务完整测试在受限环境中存在 Windows ACL/所有者操作失败，未为此削弱生产权限要求，需正常权限环境复验。

## 授权与源码

客户端、服务保留各自 GPL-3.0 授权。客户端与服务对应源码在 source ZIP，依赖来源与版本由锁文件记录。另提供 Mihomo-v1.19.32-source.zip，包含固定内核标签的完整源码。Mihomo 保持上游 GPL-3.0 授权与版本，源码：[MetaCubeX/mihomo v1.19.32](https://github.com/MetaCubeX/mihomo/tree/v1.19.32)。SimpleSC 授权与源码位于 src-tauri/packages/windows/plugins。便携包和安装包包含授权说明，未提供代码签名。

## 显式出口与网卡检测

国内 DNS 随国内有效出口，节点域名 DNS 随代理有效出口。代理可用时保留代理公共 DNS 路径；代理不可用且默认兜底可用时，非国内公共 DNS 可经默认兜底查询，已匹配代理分类的应用流量仍拒绝。备用选择只依据网卡状态、地址与网关。

首页卡片按 GUID 去重统计物理网卡，可展开虚拟网卡；只在卡片可见时每秒采样，隐藏或离开首页停止。首次采样、计数器回退、重连和长时间中断时重建基线。计数器包括所有程序与 LAN，不能用总流量代替出口核验。手动出口探测直接检测指定出口，不等同于完整应用规则路径验收。USB 内网通过 LAN 直连和系统路由访问，保持可用。

定制版使用独立版号 1.0.0。原版在线检查、静默下载和缓存安装机制已禁用，首页检查更新入口和自动检查开关已移除，只接受手动安装定制发行包。
