# MediaServo 产品参考文档

> Phase 0 → Phase 5 规划中 | 2026-07-20 | 决策数: 155+ (D1-D155)
> MVP v2 中继模型 (D118) · Host 单进程 (D155) · 基于 [架构设计文档](../architecture.md)

---

## 目录

1. [产品能力概览](./01-product-capabilities.md)
2. [部署形态](./02-deployment-modes.md)
3. [客户端与 Host](./03-client-host.md)
4. [SDK 分层与 API 参考](./04-sdk-layers.md)
5. [权限认证参考](./05-auth-permissions.md)
6. [插件体系参考](./06-plugin-system.md)
7. [协议与通信参考](./07-protocols.md)
8. [管线模型参考](./08-pipeline-model.md)
9. [传输架构](./09-transport-architecture.md)
10. [信令架构](./10-signaling-architecture.md)
11. [NAPI 绑定](./11-napi-binding.md)
12. [录制与回放](./12-recording-playback.md)
13. [服务端架构](./13-server-architecture.md)
14. [Remote 架构](./14-remote-architecture.md)
15. [Component 架构](./15-component-architecture.md)
17. [WebRTC Crate](./17-webrtc-crate.md)
18. [Codec Crate](./18-codec-crate.md)
19. [TLS Reverse Proxy & CORS 配置](./19-tls-cors-configuration.md)

**Phase 2+**: 16-admin-dashboard, operations, security-architecture, sfu-mediasoup-integration, upgrade-migration, testing-strategy, error-model, ci-cd-pipeline (创建中)

---

## 概述

MediaServo 是可独立部署的多媒体基础设施，提供七大产品能力。核心 crate：mediaservo-host（采集+编码+推流）、mediaservo-client（拉流+解码+控制）、mediaservo-server（信令+relay+监控）。Phase 1-2 采用 Host 单进程 (D155)，Phase 0-5 整体规划见架构文档。

```
工业控制端         ──┐              ┌── 宿主平台 (企业应用)
                     ├── MediaServo ──┤
   引用 native crate │  多媒体核心   │ Docker 模块
```

- **独立部署**：完整后端，自带用户/权限系统
- **平台模块**：Docker 容器，委托平台 RBAC/LDAP
- **第三方平台嵌入**：Rust crate 静态链接，仅远程桌面 + 遥操作
