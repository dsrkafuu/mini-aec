# 任务记录

## 1. 文档和契约

- [x] 1.1 将产品路径、外部 VB-CABLE 所有权和非目标同步到入口文档。
- [x] 1.2 将 AEC、实时和长时验证说明切换到 `CABLE Output`，保留 SysVAD 历史边界。
- [x] 1.3 移除生产签名和驱动发布文档，保留简化的迁移记录。
- [x] 1.4 将旧驱动目录和 release crate 标记为迁移历史并移出当前发布路径。
- [x] 1.5 将本 change 的 capability delta 同步到主规格并通过严格校验。
- [x] 1.6 清理两个未完成的生产驱动 change 及其 archive 记录，不同步其需求。
- [x] 1.7 完成端点方向、官方链接、安装所有权和旧路线引用审查。

## 2. VB-CABLE 输出实现

- [x] 2.1 将私有驱动 sink 改为项目自有 output-session 合同。
- [x] 2.2 实现只读 endpoint 枚举、精确 ID 配置和唯一 pair preflight。
- [x] 2.3 拒绝 `CABLE Output` 作为物理麦克风和 `CABLE Input` 作为物理 render。
- [x] 2.4 实现 event-driven shared-mode WASAPI render 和确定性格式转换。
- [x] 2.5 接入有界 freshest-audio 队列和输出 metadata。
- [x] 2.6 实现停止、失效、写入失败和显式 restart 的状态隔离。
- [x] 2.7 更新无窗口托盘和 headless 命令的路径、错误和输出状态。

## 3. 自动验证

- [x] 3.1 用 fake VB-CABLE output 和 endpoint inventory 替换私有驱动 fixture。
- [x] 3.2 增加 pair、格式、render、转换、underrun 和 output failure metadata。
- [x] 3.3 通过格式、workspace test、严格 Clippy、OpenSpec 和 diff 检查。

## 4. Windows 运行时验收

- [x] 4.1 固定 VB-CABLE 包、官方来源和用户手动安装/重启边界。
- [x] 4.2 通过五分钟 bypass 和普通客户端消费验证。
- [x] 4.3 完成默认 AEC 的远端单讲、近端单讲、双讲和静音恢复矩阵。
- [x] 4.4 完成 stop/start、源失效、VB-CABLE 失效和恢复验证。
- [x] 4.5 完成 K7/Realtek 的 30 分钟功能/漂移双门禁。

## 5. 旧路线清理

- [x] 5.1 删除 SysVAD、INF 模板、驱动构建/安装/签名脚本。
- [x] 5.2 删除 release/lifecycle crate 和旧 workspace surface。
- [x] 5.3 审计并移除不再使用的 SysVAD 源码和 MS-PL notice。
- [x] 5.4 删除驱动专用验证工具，保留 `vendor/VBCABLE_Driver_Pack45` 作为本地调试和手动安装材料。
- [x] 5.5 记录本地缓存和私有证据清理边界，保留 VB-CABLE 安装包目录。

## 6. 完成

- [x] 6.1 清理后重跑构建、测试、Clippy、OpenSpec 和文档检查。
- [x] 6.2 按提案、设计和规格复核产品路径、许可边界、失败行为和证据。
- [x] 6.3 同步主规格并归档 `adopt-vb-cable-output`。
