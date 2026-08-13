# MediaServo Skills Registry

## Superpowers

通过 `superpowers` 插件加载的通用技能，适用于所有项目。

## Project Skills

MediaServo 项目专属技能，位于 `.agents/skills/`：

| Skill | 类型 | 说明 |
|-------|------|------|
| `book-to-skill` | 工具 | 将书籍/文档转换为技能文件 |
| `doc-audit` | 工具 | 文档审计与一致性检查 |
| `openspec-propose` | 规范 | 创建 OpenSpec 变更提案 |
| `openspec-apply-change` | 规范 | 应用 OpenSpec 变更 |
| `openspec-explore` | 规范 | 探索已有 Spec 和变更 |
| `openspec-archive-change` | 规范 | 归档已完成的变更 |
| `openspec-sync-specs` | 规范 | 同步 Spec 与归档变更 |
| `test-harness` | 测试 | 多语言测试框架（Rust/TS/Python/C++/C） |

## 使用方式

技能由 AI 代理根据任务上下文自动激活。无需手动调用。
