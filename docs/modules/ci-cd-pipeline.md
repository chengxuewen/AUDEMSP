# CI/CD Pipeline — 持续集成/部署

> 状态：Phase 2 前设计 | 关联决策：D-CI-01, D112 | 创建依据：doc-audit M8

## 3 阶段流水线 (D-CI-01)

GitHub Actions 单 workflow，三阶段串联，阶段间传递 artifact：

```
Check → Test → Build (Package)
```

### Stage 1 — Check

运行快、无外部依赖，先行失败快速反馈：

- **rustfmt**: `cargo fmt --all -- --check`
- **clippy**: `cargo clippy --workspace --all-targets -- -D warnings`
- **cargo deny**: `cargo deny check bans licenses sources`

工具链版本由 `rust-toolchain.toml` 固定，无需矩阵。

### Stage 2 — Test

并行矩阵，覆盖全 workspace：

- `cargo test --workspace` (默认 target)
- 覆盖率采集: `cargo tarpaulin --workspace --out xml`
- Coverage report 上传到 Codecov，门禁 80%

### Stage 3 — Build & Package

- `cargo build --workspace --release`
- 3 crate 分别打包 tarball:
  - `audemsp-host-{version}-{target}.tar.gz`
  - `audemsp-client-{version}-{target}.tar.gz`
  - `audemsp-server-{version}-{target}.tar.gz`
- Artifact 上传为 workflow run attachment

## Phase 2 扩展

- **Cross-compile**: aarch64-unknown-linux-gnu (Jetson Orin) 矩阵
- **Docker**: `audemsp-server` 构建 multi-arch Docker 镜像 (linux/amd64 + linux/arm64)
- **Integration test**: 启动 docker-compose (server + 2 mock hosts)，运行 E2E 场景
- **Release automation**: tag push → 自动发布 GitHub Release + crates.io publish

## 触发规则

| 触发器 | Stage 1-2 | Stage 3 |
|--------|-----------|---------|
| PR → main | ✅ | ❌ |
| push → main | ✅ | ✅ |
| tag v* | ✅ | ✅ (release mode) |

> 详见 `.github/workflows/ci.yml`
