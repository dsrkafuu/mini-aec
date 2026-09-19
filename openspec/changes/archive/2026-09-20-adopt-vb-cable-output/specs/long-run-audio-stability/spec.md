# Spec Delta

## MODIFIED Requirements

### Requirement: 单调的长时证据
系统 SHALL 为实时 AEC run 写版本化 metadata-only 事件流，记录精确的物理麦克风、物理 render、VB-CABLE pair、请求时长、单调时间、生命周期、同步 epoch、device position/QPC、同步 delta、队列/丢弃、AEC、处理和输出计数，不记录 PCM 或会议内容。

#### Scenario: 长时记录开始
- **WHEN** run 使用精确活动的物理输入和 VB-CABLE pair 启动
- **THEN** 首个 started event 包含新的 run、同步、AEC、output-session 身份，单调时间从零开始，后续周期事件在实时线程外写入

#### Scenario: 长时记录完成
- **WHEN** 引擎达到请求时长并正常停止
- **THEN** 最后事件保留计数并标记正常完成

#### Scenario: 长时记录失败
- **WHEN** 引擎在请求时长前进入 `Failed`
- **THEN** 写入 failed event 和可操作错误，然后验证命令以失败退出

### Requirement: 30 分钟表征门禁
项目 SHALL 为 K7 物理麦克风和当前活动的 Realtek 物理 render 提供独立的 30 分钟功能稳定性门禁和时钟漂移表征门禁；路径必须经过冻结的默认 AEC3、`CABLE Input`，且普通客户端持续消费 `CABLE Output`。

#### Scenario: 表征数据有效
- **WHEN** 精确输入和 VB-CABLE pair 保持活动，漂移区间有 render packet，客户端持续消费，证据至少 30 分钟且覆盖率达标
- **THEN** 输出明确的漂移结论和独立的功能稳定性结论

#### Scenario: 活动 render 覆盖不足但功能通过
- **WHEN** 客户端持续消费、metadata 至少 30 分钟、引擎没有功能失败，但活动 render 覆盖低于漂移阈值
- **THEN** 在连续性、有界性、安全和客户端条件满足时通过功能门禁，将漂移标记为 `inconclusive`，不宣称漂移补偿或长期稳定

#### Scenario: 功能稳定性通过
- **WHEN** 区间没有终止错误、无法解释的 discontinuity/reset、旧音频、输出失败、非法 AEC、deadline miss、队列增长或未解释的客户端中断
- **THEN** 标记 30 分钟功能门禁通过，并保留所有非零的有界降级和输出计数

#### Scenario: 功能稳定性失败
- **WHEN** 提前终止或违反连续性、安全、有界性、输出或客户端条件
- **THEN** 独立于漂移结论标记功能失败，并指出首个失败条件和计数

### Requirement: 私有且不改变系统的验证边界
系统 SHALL 把事件、报告、endpoint 身份和私有录音放在被忽略的 `artifacts/`，分析 SHALL 不安装、更新、重启或移除驱动/设备，不改证书、启动配置或默认角色，不上传证据，也不发起系统重启。

#### Scenario: 分析保留证据
- **WHEN** 报告命令读取已有事件流
- **THEN** 不执行音频设备、驱动、证书、启动配置、默认角色、网络或 PCM 变更，只在批准的私有根目录写结果

#### Scenario: 产品路径不可用
- **WHEN** 真实设备门禁没有已安装的受支持 VB-CABLE pair
- **THEN** 停止并给出前置条件，不下载、安装、签名、激活设备或重启系统

#### Scenario: 测试可再分发合成证据
- **WHEN** 自动测试覆盖速率估计、discontinuity、数据不足、漂移分类和功能失败
- **THEN** 使用合成 metadata，不使用私有录音或 Windows 系统变更
