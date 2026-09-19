# OpenSpec 说明

本目录保存 MiniAEC 的当前产品规格和变更记录。当前产品路线是：物理麦克风与物理播放回环进入 AEC，结果写入 `CABLE Input`，普通客户端从 `CABLE Output` 读取。

## 目录

- `config.yaml`：项目契约、文档规则和 OpenSpec schema。
- `specs/`：当前有效的能力规格，正文使用中文，机器解析所需的 `Purpose`、`Requirements`、`Requirement` 和 `Scenario` 标题保持固定格式。
- `changes/archive/`：已完成变更的审计记录；归档后不再作为活动 change。

## 工作流

新变更必须先由用户明确要求，再按 proposal、design、specs、tasks 和实现验证推进。完成后先把 delta 合并到 `specs/`，通过 `openspec validate --all --strict --no-interactive`，再归档 change。

## 当前状态

当前规格只保留 VB-CABLE 输出路线。旧自有驱动路线的 change archive 已清理，Git 历史仍可追溯；本目录不再维护生产驱动任务。
