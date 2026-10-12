# 上游许可证检查

检查日期：2026-10-12；适用于 Clash Verge Dual NIC 1.0.0。

## 结论

可以二次修改。Clash Verge Rev 的固定提交含完整 GPL v3 许可证；本客户端的 Cargo 和 npm 元数据采用 GPL-3.0-only。本定制版继续采用 GPL v3，不改变上游版权归属，也不声称是上游官方发行版。

GPL v3 第 2 节允许制作和运行私有修改副本，无需仅因本地修改就公开仓库。向其他人分发修改版源码时，第 4、5 节要求保留版权、授权与免责说明，显著标明修改和日期，并对整个受覆盖作品采用 GPL v3。分发二进制时，第 6 节还要求提供机器可读的对应源码，包括实际修改后的客户端、服务以及控制构建/安装的脚本。向二进制接收者提供对应源码与公开托管整个仓库是不同事项；分发时应确保接收者能够取得对应版本的源码。

## 核对的组件

| 组件 | 本地证据 | 结果 |
| --- | --- | --- |
| Clash Verge Rev 2.5.8 | [根许可证](../../LICENSE)、src-tauri/Cargo.toml、package.json | GPL v3，客户端元数据为 GPL-3.0-only；允许修改及合规再分发 |
| service-ipc 2.7.6 | [服务许可证](../../vendor/service-ipc/LICENSE)、vendor/service-ipc/Cargo.toml | 完整 GPL v3；上游旧式元数据为 GPL-3.0，原样保留，不擅自扩大版本授权 |
| Mihomo 1.19.32 | [内核许可证](../../third-party/LICENSE-Mihomo.txt)、固定源码包内 LICENSE/README | GPL v3；内核未修改，分发时同时提供固定版本源码 |
| NSIS SimpleSC 1.30 | [插件授权](../../src-tauri/packages/windows/plugins/LICENSE-SimpleSC.txt)、同目录 source/ | MPL 1.1 或 LGPL 2.1 及后续版本的双重授权；授权说明和插件源码保留 |

GPL 许可证文件开头「不允许修改」针对许可证文本本身，并不是禁止修改程序。原 LICENSE 完整保留。此检查针对上述主要上游组件；锁文件内各第三方依赖及 Geo 数据仍遵循各自授权，不能把所有依赖一概视为 GPL，也不能由此推定项目名称、图标或商标授权。

## 分发材料

README 显著标明定制身份、1.0.0 版号、修改日期、修改内容和上游致谢。源码包包含客户端及修改后的 service-ipc、锁文件和构建/安装脚本；Mihomo 固定源码单独提供，打包时按构建锁核对 SHA256。插件源码保留在源码包中。便携与安装资源保留客户端、服务、内核和 SimpleSC 授权说明。对外提供安装包时，须一并向接收者提供上述对应源码；只附上游未修改的仓库链接不足以代替本定制版源码。

## 原始来源

- [固定客户端提交的 LICENSE](https://github.com/clash-verge-rev/clash-verge-rev/blob/4337bd85758d54d7b9044b2da3c9ac1176eae1f3/LICENSE)
- [固定 service-ipc 提交](https://github.com/clash-verge-rev/clash-verge-service-ipc/tree/b2a7427d6681c8d3780ab83a73f4dc21da2f22d6)
- [Mihomo 1.19.32 许可证](https://github.com/MetaCubeX/mihomo/blob/v1.19.32/LICENSE)
- [GNU GPL v3 条款](https://www.gnu.org/licenses/gpl-3.0.html)
