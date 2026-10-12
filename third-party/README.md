# 固定内核源码

本目录保留 Mihomo 的原始 GPL v3 [许可证](LICENSE-Mihomo.txt)。内核版本与提交固定在 [dualnic-build-lock.json](../dualnic-build-lock.json)。

打包前在此放置 mihomo-v1.19.32-source.zip；它对应提交 88dcbf7f1614a67c3b36b848ee3592dfa92ada36 的完整源码，SHA256 为 7e6d829b61b573cdf670c1d72af55344c805473481f10ef21cb2cfbbf36eb878。若发行资产中的文件名为 Mihomo-v1.19.32-source.zip，复制到此目录时使用上述小写文件名。

源码归档不纳入 Git。分发二进制时必须同时向接收者提供对应源码；打包脚本会验证归档存在且哈希匹配。仓库无需保留重复的内核 Git 克隆，也不以最新版或 Alpha 内核替换固定版本。

上游：[MetaCubeX/mihomo v1.19.32](https://github.com/MetaCubeX/mihomo/tree/v1.19.32)。
