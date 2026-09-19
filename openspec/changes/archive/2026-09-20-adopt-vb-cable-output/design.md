# 设计说明

## 决策摘要

MiniAEC 的支持路径固定为：物理麦克风和物理播放回环进入冻结的 M131 AEC3，处理结果经用户态输出边界写入 `CABLE Input`，普通客户端从 `CABLE Output` 读取。SysVAD 不再是可选 fallback。

## 关键决策

1. VB-CABLE 是唯一支持的外部输出依赖，不扩展到 Voicemeeter 或其他虚拟线缆。
2. 用户从官方来源直接安装和管理 VB-CABLE；仓库可保留用户提供的官方安装包供本地调试和构建后手动安装，但 release 不包含该安装包。产品代码只做只读 endpoint preflight 和普通音频运行，不请求安装权限、不接受许可证、不发起重启。
3. 选择使用精确 endpoint ID、data-flow role 和设备元数据，友好名称只用于诊断；pair 缺失或歧义时 fail closed。
4. 输出使用 event-driven shared-mode WASAPI render；引擎提供完整的 10 ms/48 kHz mono finite frame，Windows 适配器负责实际 mix format 转换。
5. 输出初始化、端点失效、写入失败或有界队列失败会终止当前 run，清空所有 PCM，进入 `Failed`，只允许显式 restart。
6. 验收面是 `CABLE Input -> CABLE Output`，由用户操作 Windows Recorder 和至少一个目标会议客户端；自动验证不修改系统。
7. VB-CABLE 验收通过后删除旧 SysVAD、驱动构建/签名/安装脚本和 release crate；Git 历史和保留的迁移摘要承担必要的追溯作用。

## 风险处理

- 端点改名或多设备：保存精确 ID，并用角色和元数据校验，无法唯一配对就失败。
- 输入输出方向混淆：文档、托盘和错误信息始终显示 `MiniAEC -> CABLE Input -> CABLE Output -> 客户端`。
- 输出时钟和队列问题：记录 padding、clock、转换、underrun 和 discard，并使用 30 分钟功能/漂移双门禁。
- 外部许可变化：仓库保留的安装包只用于本地调试和构建后手动安装，不进入 release；任何正式分发变化另开评审。

## 迁移和回滚

迁移顺序是先改契约和规格，再实现输出适配器，再做合成/真机验收，最后清理旧驱动。清理后不恢复测试模式、开发驱动或系统重启；需要调查历史问题时使用 Git 历史和保留的迁移摘要。
