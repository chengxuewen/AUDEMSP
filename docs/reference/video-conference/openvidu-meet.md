# OpenVidu Meet 客户端架构参考

> 生成日期：2026-07-31 | 分类：视频会议
> 仓库：https://github.com/OpenVidu/openvidu (openvidu-call)

## 1. 概述

OpenVidu Meet（官方名称 OpenVidu Call）是 OpenVidu 平台的官方旗舰 Web 客户端应用。基于 React 的 WebRTC 视频会议应用，展示 openvidu-browser SDK 的全部能力。定义了 OpenVidu 生态中客户端应用的最佳实践架构：连接管理、媒体管线、UI 组件编排、网络容错。

### 1.1 三层架构

```
UI 层：React 组件 (视频网格/工具栏/聊天面板/设置)
状态管理：React Context + useReducer
        │
业务逻辑层：openvidu-browser SDK
Session / Publisher / Subscriber / StreamManager
事件驱动：streamCreated, connectionCreated, signal
        │ REST (HTTPS) + WebSocket (WSS)
openvidu-server (Java Spring Boot)
REST API: session/connection/token CRUD
WebSocket: SDP 协商, ICE 交换, 信令消息
```

OpenVidu Meet 不直接操作 WebRTC API。所有 WebRTC 交互通过 openvidu-browser SDK 封装，SDK 内部管理 RTCPeerConnection、ICE 候选收集、SDP 协商。客户端开发者只需调用 Session.connect()、Session.publish()、Session.subscribe() 等高层方法。

### 1.2 与 openvidu-server 的接口边界

| 接口 | 协议 | 用途 | 客户端使用方 |
|------|------|------|-------------|
| REST API | HTTPS | 创建 Session、获取 Token | 应用服务器（非客户端直接调用） |
| WebSocket | WSS | SDP 协商、ICE 交换、信令消息 | openvidu-browser SDK |
| TURN/STUN | UDP/TCP | ICE 候选收集 | 浏览器原生 WebRTC |

关键设计：客户端从不直接调用 REST API。Token 由应用服务器从 openvidu-server 获取后传递给客户端。客户端仅通过 WebSocket 与 openvidu-server 通信。

## 2. 技术栈

### 2.1 核心依赖

| 层级 | 技术 | 版本 | 用途 |
|------|------|------|------|
| 框架 | React | 18.x | UI 组件架构 |
| 语言 | TypeScript | 5.x | 类型安全 |
| 构建 | Vite 或 CRA | 5.x | 开发/生产构建 |
| WebRTC SDK | openvidu-browser | 2.31.0 | WebRTC 封装 |
| 样式 | CSS Modules + SCSS | - | 组件样式隔离 |
| 状态管理 | React Context + useReducer | - | 全局状态 |
| 路由 | React Router | 6.x | 页面路由 |

### 2.2 openvidu-browser SDK 核心类

- **OpenVidu**: 入口点工厂，创建 Session/Publisher
- **Session**: 视频会议会话，核心方法 connect/publish/subscribe/disconnect/signal，事件驱动 (streamCreated, connectionCreated, sessionDisconnected)
- **Publisher** (extends StreamManager): 本地媒体发布，方法 publishAudio/publishVideo/replaceTrack，事件 accessAllowed/accessDenied
- **Subscriber** (extends StreamManager): 远程媒体接收，事件 videoElementCreated/streamPlaying
- **StreamManager** 基类: stream 属性、videos 数组、addVideoElement/createVideoElement 视频元素管理

### 2.3 状态管理

OpenVidu Meet 使用 React Context + useReducer 管理全局状态，不引入 Redux 或 MobX：

```typescript
interface CallState {
  session: Session | null;
  connectionStatus: 'disconnected' | 'connecting' | 'connected';
  participants: Map<string, Participant>;
  publisher: Publisher | null;
  subscribers: Map<string, Subscriber>;
  audioEnabled: boolean;
  videoEnabled: boolean;
  isScreenSharing: boolean;
  chatMessages: ChatMessage[];
  activeSpeaker: string | null;
}
```

Context 通过 Provider 注入组件树，子组件通过 useContext 读取状态，通过 dispatch 更新。

## 3. 连接流程

### 3.1 完整连接生命周期

```
客户端                        应用服务器                   openvidu-server
 1. 请求加入房间 ─────────────►
                              2. POST /api/sessions ────────►
                              3. 返回 sessionId ◄──────────
                              4. POST /api/sessions/{id}/connection ──►
                              5. 返回 token ◄───────────────
 6. 返回 token ◄──────────────
 7. new OpenVidu() + OV.initSession()
 8. session.on('streamCreated', ...)
 9. session.connect(token) ─────────────────────────────────►
                   ◄── WebSocket 连接建立 ──►
10. session.on('connectionCreated')
11. OV.initPublisher() + session.publish(publisher) ────────►
                   ◄── SDP offer/answer ──────►
                   ◄── ICE 候选交换 ──────────►
12. streamCreated 事件 (远程) → session.subscribe(stream)
13. 视频元素添加到 DOM 并播放
```

### 3.2 Token 获取（应用服务器端）

客户端不直接调用 openvidu-server REST API。应用服务器负责通过 openvidu-java-client 或 openvidu-node-client SDK 创建 Session 和 Connection，并返回 token 给客户端。Token 是 JWT 字符串，包含 sessionId、connectionId、角色和权限信息。客户端通过 WebSocket 连接时携带 token 进行身份验证：

```
wss://openvidu.example.com?sessionId=ses_xxx&token=wss://....
```

## 4. 视频/音频管线

### 4.1 发布管线 (Publisher)

```
getUserMedia → MediaStream
    → OV.initPublisher(targetElement, properties)
        → 创建本地 <video> 预览，绑定 MediaStream
        → 等待 accessAllowed 事件
    → session.publish(publisher)
        → RTCPeerConnection 创建，addTrack
        → createOffer → setLocalDescription
        → SDP 经 WebSocket 发送到服务器
        → 接收 answer → setRemoteDescription
        → ICE 候选收集与交换
    → 发布成功，streamCreated 事件触发
```

后续控制：publishAudio(false) 静音，publishVideo(false) 关闭摄像头，replaceTrack(newTrack) 切换摄像头/屏幕共享。

### 4.2 订阅管线 (Subscriber)

远程参与者发布流后，streamCreated 事件触发：

```
session.on('streamCreated', (event) => {
    const subscriber = session.subscribe(event.stream, 'video-container', { insertMode: 'APPEND' });
    subscriber.on('videoElementCreated', (event) => { /* 视频元素已加入 DOM */ });
    subscriber.on('streamPlaying', () => { /* 远程视频开始播放，更新布局 */ });
});
```

### 4.3 视频元素管理

两种模式：SDK 自动管理（传入 targetElement，SDK 自动创建 `<video>` 并追加）和手动管理（传入 null，调用 subscriber.addVideoElement() 或 createVideoElement()）。手动模式适用于自定义视频网格布局库。

### 4.4 屏幕共享

使用 replaceTrack 替换视频轨道，无需重新协商 WebRTC 连接：

```typescript
const screenStream = await navigator.mediaDevices.getDisplayMedia({ video: true });
const videoTrack = screenStream.getVideoTracks()[0];
await publisher.replaceTrack(videoTrack);
```

## 5. 房间管理

### 5.1 Session 生命周期

```
创建 OpenVidu → initSession() → 注册事件监听器
    → connect(token) → 已连接（publish/subscribe/signal）
        → disconnect() → 已断开
```

### 5.2 参与者追踪

通过 Session 事件实时维护参与者 Map：

```typescript
// connectionCreated → 添加参与者
// connectionDestroyed → 移除参与者
// streamCreated → 关联 stream 到参与者
// streamDestroyed → 解除关联
```

参与者数据结构包含 connectionId、nickname、isLocal、audioEnabled、videoEnabled、stream、subscriber、joinTime。

### 5.3 离开房间

```typescript
function leaveRoom() {
  session.disconnect();
  // 发布者自动取消发布，订阅者自动取消订阅（SDK 内部处理）
  // sessionDisconnected 事件触发
  dispatch({ type: 'RESET' }); // 清理参与者列表
  navigate('/'); // 导航回主页
}
```

## 6. UI 组件架构

### 6.1 组件树

```
<App>
  └── <CallProvider> (Context Provider)
        <RoomPage>
          ├── <PreJoinScreen> (设备预览/名称设置)
          ├── <CallContainer>
          │   ├── <VideoGrid> → <VideoTile> × n
          │   │   <video> + <ParticipantInfo> + <AudioIndicator>
          │   ├── <Toolbar>
          │   │   <MicButton> <CamButton> <ScreenShareButton>
          │   │   <ChatButton> <SettingsButton> <HangupButton>
          │   ├── <ChatPanel> (<MessageList> + <MessageInput>)
          │   └── <ParticipantsPanel>
          └── <SettingsModal> (<AudioSettings> <VideoSettings> <GeneralSettings>)
```

### 6.2 VideoGrid 布局策略

| 参与者数 | 布局模式 |
|---------|---------|
| 1 | 全屏 |
| 2 | 并排 50%/50% |
| 3-4 | 2x2 网格 |
| 5-6 | 3x2 不等分，主发言者稍大 |
| 7+ | 动态网格，滚动 + 突出活跃发言者 |

布局计算使用纯 CSS Grid 或 absolute 定位，不依赖第三方布局库：

```typescript
function calculateGridLayout(count: number) {
  if (count <= 1) return { columns: 1, rows: 1 };
  if (count <= 2) return { columns: 2, rows: 1 };
  if (count <= 4) return { columns: 2, rows: 2 };
  const columns = Math.ceil(Math.sqrt(count));
  const rows = Math.ceil(count / columns);
  return { columns, rows };
}
```

### 6.3 工具栏与媒体状态的同步

工具栏按钮状态通过 dispatch 同步到 SDK，useCallback 包装避免重渲染：

```typescript
const toggleMic = useCallback(async () => {
  await publisher.publishAudio(!audioEnabled);
  dispatch({ type: 'SET_AUDIO', payload: !audioEnabled });
}, [publisher, audioEnabled]);
```

## 7. 网络容错

### 7.1 WebSocket 重连

| 场景 | 行为 | 用户体验 |
|------|------|---------|
| 临时网络中断 | 自动重连，最多 5 次，指数退避 | 短暂冻结后恢复 |
| 重连超时 | sessionDisconnected 事件触发 | 提示用户重新加入 |
| ICE 连接失败 | ICE restart 触发 | 短暂黑屏后恢复 |

指数退避：基础 1 秒，最大 30 秒，退避因子 2x，抖动 ±25%，最大重试 5 次。

### 7.2 ICE Restart 与连接质量监控

服务器端检测到连接质量下降时触发 ICE restart，客户端通过 reconnecting/reconnected 事件感知状态变化。

openvidu-browser 通过 RTCPeerConnection.getStats() 每 2 秒采集连接质量（RTT、丢包率、抖动、比特率、分辨率），根据质量自动调整。

## 8. openvidu-server API 消费

### 8.1 REST API 端点（应用服务器使用）

| 方法 | 路径 | 用途 |
|------|------|------|
| POST | /openvidu/api/sessions | 创建会话 |
| POST | /openvidu/api/sessions/{id}/connection | 创建连接（生成 Token） |
| DELETE | /openvidu/api/sessions/{id} | 关闭会话 |
| GET | /openvidu/api/sessions/{id} | 获取会话信息 |
| POST | /openvidu/api/recordings/start | 开始录制 |
| POST | /openvidu/api/recordings/stop | 停止录制 |
| PATCH | /openvidu/api/sessions/{id}/connection/{connId} | 更新连接属性 |

### 8.2 WebSocket 消息类型

| C→S | S→C |
|-----|-----|
| joinRoom | participantJoined |
| publishVideo | participantLeft |
| receiveVideoFrom | participantPublished |
| unpublishVideo | participantUnpublished |
| unsubscribeFrom | mediaError |
| sendMessage | iceCandidate |
| iceCandidate | connectionCreated |
| leaveRoom | connectionDestroyed |

### 8.3 信号消息（自定义信令）

OpenVidu 支持通过 session.signal() 发送自定义消息，实现聊天等功能：

```typescript
// 发送
await session.signal({ type: 'chat', data: JSON.stringify({ message, timestamp, sender }), to: [] });
// 接收
session.on('signal:chat', (event) => { /* 解析 event.data */ });
```

不需要额外的 WebSocket 连接，消息类型通过 type 字段区分，支持定向发送和广播。

## 9. OMSPBase 可借鉴的设计

### 9.1 Session 抽象层

客户端不直接操作 RTCPeerConnection，所有 WebRTC 细节由 SDK 封装。OMSPBase Admin Dashboard 可以定义 `MeetingSession` 接口，封装 join/leave/publish/subscribe，底层实现可切换（P2P、SFU-mediasoup、SFU-LiveKit），对 UI 层完全透明。

### 9.2 Token 鉴权流程

应用服务器生成 Token，客户端不接触 REST API 密钥。每个 Token 绑定特定 Session 和 Connection，支持角色权限控制（PUBLISHER/SUBSCRIBER），单次使用防重放。OMSPBase 的 SFU 鉴权可以借鉴此设计，替代当前简单的 PSK 模型。

### 9.3 Subscriber 自动管理 + 信号消息通道

streamCreated 事件自动触发 subscribe，streamDestroyed 自动触发 unsubscribe，事件驱动而非轮询。OMSPBase 的 SFU consume 流程可以封装为类似的响应式模式。

signal() 机制提供轻量级自定义消息通道，不需要额外连接。OMSPBase 的 WebSocket 信令协议可以增加类似的通用信号通道。

### 9.4 视频元素管理分离

媒体层负责获取/发布/订阅流，渲染层负责将流绑定到 DOM 元素。渲染层可使用任何布局库，不受媒体层约束。

### 9.5 差距分析

| 维度 | OpenVidu Meet | OMSPBase Admin Dashboard |
|------|---------------|-------------------------|
| SDK 封装 | 完整（openvidu-browser） | 无（直接操作 mediasoup-client） |
| Session 管理 | Session 对象 + 事件驱动 | 手动管理 WebSocket 消息 |
| 参与者追踪 | 内置（connectionCreated/Destroyed） | 手动维护 |
| 视频元素管理 | 自动/手动双模式 | 需自行实现 |
| 信号消息 | 内建 signal() 通道 | 需自定义 WS 消息 |
| 重连策略 | 内置指数退避 | 无 |
| 连接质量 | 内置 getStats 监控 | 无 |
| 设备管理 | getDevices() + 自动切换 | 需自行实现 |
| 屏幕共享 | replaceTrack 一行切换 | 需自行实现 |
| 录制 | REST API 一行触发 | 未实现 |

OpenVidu Meet 是成熟的参考实现，OMSPBase Admin Dashboard 可以从中提取大量设计模式，避免重造轮子。