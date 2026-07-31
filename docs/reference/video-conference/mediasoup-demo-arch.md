# mediasoup-demo Architecture Deep Dive

> 源码分析: [versatica/mediasoup-demo](https://github.com/versatica/mediasoup-demo) v3 tag
> 分析日期: 2026-07-31

## 1. 整体架构

```
┌──────────────────────────────────────────────────────────────────┐
│                        Client Browser (app/)                      │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────────┐  │
│  │ mediasoup-   │   │ protoo-      │   │  Redux Store         │  │
│  │ client       │   │ client       │   │  (state management)  │  │
│  │ (Device/     │   │ (WebSocket)  │   └──────────────────────┘  │
│  │  Transport)  │   └──────┬───────┘                             │
│  └──────┬───────┘          │                                      │
│         │ ICE/DTLS/SRTP    │ WebSocket (JSON)                     │
│         │                  │                                      │
└─────────┼──────────────────┼──────────────────────────────────────┘
          │                  │
          │   ┌──────────────┼───────────────────────────────┐
          │   │          Server (server/)                     │
          │   │  ┌──────────────────────────────┐            │
          │   │  │  WsServer (protoo-server)    │            │
          │   │  │  ┌────────────────────────┐  │            │
          │   │  │  │  protoo.Room           │  │            │
          │   │  │  │  ├─ Peer (browser 1)   │  │            │
          │   │  │  │  ├─ Peer (browser 2)   │  │            │
          │   │  │  │  └─ Peer (browser N)   │  │            │
          │   │  │  └────────────────────────┘  │            │
          │   │  └──────────┬───────────────────┘            │
          │   │             │ events                         │
          │   │  ┌──────────▼───────────────────┐            │
          │   │  │     Room                     │            │
          │   │  │  ┌───────────────────────┐   │            │
          │   │  │  │ producerRouter         │   │            │
          │   │  │  │ consumerRouter         │   │            │
          │   │  │  │ producerWebRtcServer   │   │            │
          │   │  │  │ consumerWebRtcServer   │   │            │
          │   │  │  │ AudioLevelObserver     │   │            │
          │   │  │  │ ActiveSpeakerObserver  │   │            │
          │   │  │  │ Bot                    │   │            │
          │   │  │  └───────────────────────┘   │            │
          │   │  └──────────┬───────────────────┘            │
          │   └──────────────┼───────────────────────────────┘
          │                  │
          │   ┌──────────────▼───────────────────────────────┐
          │   │         mediasoup Workers (N = CPU cores)     │
          │   │  ┌───────────┐  ┌───────────┐  ┌───────────┐ │
          │   │  │ Worker[0] │  │ Worker[1] │  │ Worker[N] │ │
          │   │  │┌─────────┐│  │┌─────────┐│  │┌─────────┐│ │
          │   │  ││Router   ││  ││Router   ││  ││Router   ││ │
          │   │  ││(producer││  ││(consumer││  ││Router   ││ │
          │   │  ││ side)   ││  ││ side)   ││  ││         ││ │
          │   │  │├─────────┤│  │├─────────┤│  │├─────────┤│ │
          │   │  ││WebRtc   ││  ││WebRtc   ││  ││WebRtc   ││ │
          │   │  ││Server   ││  ││Server   ││  ││Server   ││ │
          │   │  │└─────────┘│  │└─────────┘│  │└─────────┘│ │
          │   │  └───────────┘  └───────────┘  └───────────┘ │
          │   └───────────────────────────────────────────────┘
          └──────────────────────────────────────────────────────┘
```

### 核心分层

| 层 | 组件 | 职责 |
|----|------|------|
| 信令层 | protoo-server / WsServer | WebSocket 连接管理，JSON 消息路由 |
| 房间逻辑层 | Room | Producer/Consumer 编排，peer 生命周期 |
| 端点抽象层 | Peer / BroadcasterPeer | 单个端点的 transport/producer/consumer 管理 |
| 媒体路由层 | mediasoup Worker + Router | 实际媒体包转发 |
| 客户端 SDK | mediasoup-client + protoo-client | 浏览器端 Device/Transport/Producer/Consumer |

## 2. 启动与多 Worker 配置

### 2.1 进程入口

```
index.ts → getConfig() → Server.create(config) → server 监听
```

### 2.2 Worker 创建策略

```typescript
// Server.ts: createWorkersAndWebRtcServers()
const numWorkers = config.mediasoup.numWorkers; // 默认 = os.cpus().length

for (let idx = 0; idx < numWorkers; ++idx) {
    const worker = await mediasoup.createWorker({ ...workerSettings });

    // 每个 Worker 绑定一个 WebRtcServer，端口递增
    const webRtcServerOptions = { ...baseWebRtcServerOptions };
    for (const listenInfo of webRtcServerOptions.listenInfos) {
        listenInfo.port += idx;  // Worker 0: 44444, Worker 1: 44445, ...
    }

    const webRtcServer = await worker.createWebRtcServer(webRtcServerOptions);
    workersAndWebRtcServers.set(idx, { worker, webRtcServer });
}
```

关键点:
- **Worker 数 = CPU 核数**
- **每个 Worker 一个 WebRtcServer**，端口递增（44444, 44445, ...）
- WebRtcServer 同时监听 UDP 和 TCP

### 2.3 默认配置

```javascript
// config.example.mjs
mediasoup: {
    numWorkers: Object.keys(os.cpus()).length,
    workerSettings: {
        logLevel: 'warn',
        // 丰富的 logTags: info, ice, dtls, rtp, srtp, rtcp, rtx, bwe, score...
    },
    webRtcServerOptions: {
        listenInfos: [
            { protocol: 'udp', ip: '0.0.0.0', port: 44444 },
            { protocol: 'tcp', ip: '0.0.0.0', port: 44444 },
        ],
    },
    webRtcTransportOptions: {
        initialAvailableOutgoingBitrate: 1_000_000,
        minimumAvailableOutgoingBitrate: 600_000,
        maxSendMessageSize: 5_000_000,
        sctpSendBufferSize: 7_000_000,
        enableSctp: true,
    },
    routerOptions: {
        mediaCodecs: [
            { kind: 'audio', mimeType: 'audio/opus', clockRate: 48000, channels: 2 },
            { kind: 'video', mimeType: 'video/VP8', clockRate: 90000 },
            { kind: 'video', mimeType: 'video/H264', clockRate: 90000,
              parameters: { 'packetization-mode': 1, 'profile-level-id': '42e01f' } },
            // VP9, AV1, H265...
        ],
    },
}
```

### 2.4 listenIps vs webRtcServer

mediasoup-demo 使用 **webRtcServer 模式**（不是旧的 listenIps 模式）:

- `webRtcServer` 是一个独立对象，管理 UDP/TCP socket
- `WebRtcTransport` 创建时引用 `webRtcServer`，不直接配置 listenIps
- 好处: 多个 Transport 共享同一组 UDP/TCP 端口，ICE 复用

## 3. Room 创建和 Router 分配

### 3.1 Room 按需创建

```
Client WS connect → WsServer → Server.getOrCreateRoom(roomId)
```

```typescript
// Server.ts: getOrCreateRoom()
// 使用 AwaitQueue 防止 race condition（多用户同时 join 同一 room）
await roomData.queue.push(async () => {
    if (roomData.room) return roomData.room; // 已有，直接返回

    // 分配 producer Worker (round-robin)
    const { worker: producerWorker, webRtcServer: producerWebRtcServer } =
        this.getNextWorkerAndWebRtcServer();

    // usePipeTransports=true 时分配另一个 consumer Worker
    const { worker: consumerWorker, webRtcServer: consumerWebRtcServer } =
        usePipeTransports
            ? this.getNextWorkerAndWebRtcServer()
            : { worker: producerWorker, webRtcServer: producerWebRtcServer };

    // 创建 Router(s)
    const producerRouter = await producerWorker.createRouter({ mediaCodecs });
    const consumerRouter = usePipeTransports
        ? await consumerWorker.createRouter({ mediaCodecs })
        : producerRouter; // 单 Worker: 生产者和消费者共用同一个 Router

    const room = await Room.create({ producerRouter, consumerRouter, ... });
});
```

### 3.2 Producer / Consumer Router 分离

```
单 Worker (usePipeTransports=false):
  ┌───────────────────┐
  │   Worker W0       │
  │  ┌─────────────┐  │
  │  │  Router R0  │  │ ← Producer + Consumer 共享
  │  │  WebRtcSvr  │  │
  │  └─────────────┘  │
  └───────────────────┘

多 Worker (usePipeTransports=true):
  ┌──────────────┐     pipeToRouter    ┌──────────────┐
  │  Worker W0   │ ←────────────────── │  Worker W1   │
  │ ┌──────────┐ │                     │ ┌──────────┐ │
  │ │ Router R0│ │   (pipeTransport)   │ │ Router R1│ │
  │ │(producer)│ │───────────────────→ │ │(consumer)│ │
  │ │WebRtcSvr │ │                     │ │WebRtcSvr │ │
  │ └──────────┘ │                     │ └──────────┘ │
  └──────────────┘                     └──────────────┘
```

**Room 接收参数**:
```typescript
type RoomCreateOptions = {
    roomId: RoomId;
    consumerReplicas: number;         // Consumer 副本数 (0=无副本)
    usePipeTransports: boolean;       // 是否使用跨 Worker PipeTransport
    disableBwe: boolean;              // 是否禁用 BWE RTP 扩展
    config: ServerConfig;
    producerRouter: mediasoup.Router;
    consumerRouter: mediasoup.Router;
    producerWebRtcServer: mediasoup.WebRtcServer;
    consumerWebRtcServer: mediasoup.WebRtcServer;
};
```

## 4. Client Join 完整信令序列

### 4.1 时序图

```
Browser (RoomClient)                  Server (WsServer → Room → Peer)
        │                                       │
        │─── WebSocket connect ────────────────→│ 查询参数: roomId, peerId
        │                                       │
        │←─── "mediasoupVersion" notification ──│ { version: "3.x.x" }
        │                                       │
   [创建 mediasoupClient.Device]                │
        │                                       │
        │─── request: "getRtcStatsUrl" ────────→│
        │←─── response: { rtcstatsUrl } ────────│
        │                                       │
        │─── request: "getRouterRtpCapabilities"│
        │←─── response: { routerRtpCapabilities }│
        │                                       │
   [device.load({ routerRtpCapabilities })]     │
        │                                       │
   [getUserMedia hack for autoplay policy]     │
        │                                       │
   [if produce: 创建 send Transport]            │
   [if consume: 创建 recv Transport]            │
        │                                       │
        │─── request: "createWebRtcTransport" ─→│ { forceTcp, appData: { direction } }
        │                                       │  Server: router.createWebRtcTransport({
        │                                       │    webRtcServer, enableUdp, enableTcp,
        │                                       │    iceConsentTimeout: 20, enableSctp: true
        │                                       │  })
        │←─── response: {                       │
        │       id, iceParameters,              │
        │       iceCandidates, dtlsParameters,   │
        │       sctpParameters                   │
        │     }                                  │
        │                                       │
   [client.createSendTransport({...})]          │
        │                                       │
        │───  "connect" event ──────────────────│ DTLS handshake
        │─── request: "connectWebRtcTransport" ─→│ transport.connect({ dtlsParameters })
        │←─── accept ──────────────────────────│
        │                                       │
   [如 consume: 同样流程 for recvTransport]      │
        │                                       │
        │─── request: "join" ───────────────────→│ { displayName, device, rtpCapabilities }
        │←─── response: { peers: [...] } ────────│ 当前在房间的其他 peer 列表
        │                                       │
   [状态 → 'connected']                         │
   [对 peers 中每个 peer 通知 newPeer]          │
   [对已有 peer 的 producer 调用 consume]        │
        │                                       │
        │←─── notification: "newPeer" ────────────│ (广播给其他 peer)
        │                                       │
```

### 4.2 关键代码路径

**客户端 `_joinRoom()`** (RoomClient.js:2327):

```javascript
async _joinRoom() {
    // 1. 创建设备
    this._mediasoupDevice = await mediasoupClient.Device.factory({ handlerName });

    // 2. 获取 Router RTP capabilities
    const { routerRtpCapabilities } = await this._protoo.request('getRouterRtpCapabilities');
    await this._mediasoupDevice.load({ routerRtpCapabilities });

    // 3. 创建 send Transport (direction: 'producer')
    const transportInfo = await this._protoo.request('createWebRtcTransport', {
        forceTcp: this._forceTcp,
        appData: { direction: 'producer' },
    });
    this._sendTransport = this._mediasoupDevice.createSendTransport({...});

    this._sendTransport.on('connect', ({ dtlsParameters }, callback) => {
        this._protoo.request('connectWebRtcTransport', {
            transportId: this._sendTransport.id,
            dtlsParameters,
        }).then(callback);
    });

    // 4. 创建 recv Transport (direction: 'consumer') — 如果 consume=true
    const recvInfo = await this._protoo.request('createWebRtcTransport', {
        forceTcp: this._forceTcp,
        appData: { direction: 'consumer' },
    });
    this._recvTransport = this._mediasoupDevice.createRecvTransport({...});

    // 5. Join room
    const { peers } = await this._protoo.request('join', {
        displayName, device,
        rtpCapabilities: this._consume ? this._mediasoupDevice.rtpCapabilities : undefined,
    });

    store.dispatch(stateActions.setRoomState('connected'));
}
```

**服务端 Peer join 处理** (Peer.ts):

```typescript
case 'join': {
    const { displayName, device, rtpCapabilities } = data;
    this.#joined = true;
    this.#displayName = displayName;
    this.#device = device;
    this.#rtpCapabilities = rtpCapabilities;

    clearTimeout(this.#joinTimer); // 取消 30s 超时

    this.emit('joined', serializedPeers => {
        accept({ peers: serializedPeers }); // 返回同房间其他 peer
    });
}
```

**Peer 超时机制**: Peer 创建时启动 30s join 定时器，超时自动 close。

## 5. WebRtcTransport 创建

### 5.1 服务端处理

```typescript
// Room.ts: handlePeer() → 'create-web-rtc-transport' 事件
peer.on('create-web-rtc-transport', async ({ direction, forceTcp }, resolve, reject) => {
    // 根据 direction 选择对应的 Router 和 WebRtcServer
    const mediasoupRouter = (direction === 'producer')
        ? this.#producerRouter
        : this.#consumerRouter;

    const mediasoupWebRtcServer = (direction === 'producer')
        ? this.#producerWebRtcServer
        : this.#consumerWebRtcServer;

    const transport = await mediasoupRouter.createWebRtcTransport({
        ...this.#config.mediasoup.webRtcTransportOptions,
        enableUdp: !forceTcp,
        enableTcp: true,
        webRtcServer: mediasoupWebRtcServer,  // 关键: 绑定到 WebRtcServer
        iceConsentTimeout: 20,
        enableSctp: true,
        appData: { direction },
    });

    resolve(transport);
});
```

### 5.2 Transport 参数说明

| 参数 | 值 | 说明 |
|------|-----|------|
| `webRtcServer` | Worker 绑定的 WebRtcServer | ICE 复用，单端口多 Transport |
| `enableUdp` | `!forceTcp` | 默认 true |
| `enableTcp` | `true` | 始终启用 TCP fallback |
| `iceConsentTimeout` | `20` | ICE consent 刷新超时(秒) |
| `enableSctp` | `true` | 启用 DataChannel |
| `initialAvailableOutgoingBitrate` | 1_000_000 | 初始出站码率 |

### 5.3 Producer 和 Consumer 使用不同 Transport

- **sendTransport** (direction='producer'): 绑定 producerRouter + producerWebRtcServer
- **recvTransport** (direction='consumer'): 绑定 consumerRouter + consumerWebRtcServer

如果是单 Worker (usePipeTransports=false)，两者指向同一个 Router 和 WebRtcServer。

## 6. Produce / Consume 信令交互

### 6.1 Producer 创建

```
Browser                           Server (Peer → Room)
  │                                       │
  │─── _sendTransport.on('produce')      │
  │    (mediasoup-client 自动触发)         │
  │                                       │
  │─── request: "produce" ──────────────→│ { transportId, kind, rtpParameters, appData }
  │                                       │
  │                                       │ transport.produce({ kind, rtpParameters,
  │                                       │   appData: { peerId, source } })
  │                                       │
  │←─── response: { producerId } ─────────│
  │                                       │
  │                                       │ emit 'new-producer' → Room
  │                                       │
  │                                       │ Room: if usePipeTransports →
  │                                       │   producerRouter.pipeToRouter({
  │                                       │     producerId, router: consumerRouter
  │                                       │   })
  │                                       │
  │                                       │ Room: for each otherPeer →
  │                                       │   otherPeer.consume({ producer })
  │                                       │
  │                                       │ if audio:
  │                                       │   audioLevelObserver.addProducer()
  │                                       │   activeSpeakerObserver.addProducer()
```

**客户端 produce 触发**:
```javascript
// RoomClient.js: sendTransport.on('produce')
this._sendTransport.on('produce', async ({ kind, rtpParameters, appData }, callback) => {
    const { producerId } = await this._protoo.request('produce', {
        transportId: this._sendTransport.id,
        kind, rtpParameters, appData,
    });
    callback({ id: producerId });
});

// 实际调用 (enableMic/enableWebcam 时)
this._micProducer = await this._sendTransport.produce({
    track: audioTrack, appData: { source: 'webcam' }
});
this._webcamProducer = await this._sendTransport.produce({
    track: videoTrack, appData: { source: 'webcam' }
});
```

### 6.2 Consumer 创建

```
Server (Peer.consume)                     Browser
  │                                       │
  │ 1. 检查 canConsume:                    │
  │    router.canConsume({ producerId,     │
  │      rtpCapabilities })                │
  │                                       │
  │ 2. 创建 Consumer (paused):             │
  │    consumer = await transport          │
  │      .consume({ producerId,            │
  │        rtpCapabilities,                │
  │        enableRtx: true,                │
  │        paused: true,                   │
  │        ignoreDtx: true })              │
  │                                       │
  │ 3. 通知客户端:                          │
  │─── notification: "newConsumer" ───────→│ { peerId, transportId, consumerId,
  │                                       │   producerId, kind, rtpParameters,
  │                                       │   type, producerPaused, consumerScore,
  │                                       │   appData }
  │                                       │
  │                                       │ 4. 客户端创建 local Consumer:
  │                                       │    consumer = await
  │                                       │      recvTransport.consume({
  │                                       │        id: consumerId,
  │                                       │        producerId, kind,
  │                                       │        rtpParameters, appData })
  │                                       │
  │                                       │ 5. on success:
  │─── request: "resume" ←────────────────│
  │                                       │
  │ 6. await consumer.resume()            │
  │                                       │
  │                                       │ 7. consumer.on('track')
  │                                       │    → video.srcObject = stream
```

**客户端 newConsumer 处理** (RoomClient.js:387):
```javascript
case 'newConsumer': {
    const { peerId, producerId, consumerId, kind, rtpParameters, type,
            producerPaused, consumerScore, appData } = request;

    const consumer = await this._recvTransport.consume({
        id: consumerId, producerId, kind, rtpParameters, appData,
    });

    // consumer.on('track') → 渲染到 UI
    // 连接 resume: await this._protoo.request('resume', { consumerId });
}
```

### 6.3 Consumer Replicas

```typescript
// Peer.ts: consume()
const consumerCount = 1 + consumerReplicas; // 默认 0 → 1 个 Consumer

for (let i = 0; i < consumerCount; ++i) {
    // 每个 consumer 独立创建，可能在不同 transport
    const consumer = await transport.consume({ ... });
    await this.request('newConsumer', { ... });
    await consumer.resume();
}
```

### 6.4 Pipe Transport (usePipeTransports=true)

```
Producer (Worker 0)                    Consumer (Worker 1)
       │                                       │
       │  producerRouter.pipeToRouter({        │
       │    producerId,                        │
       │    router: consumerRouter             │
       │  })                                   │
       │                                       │
       │  ──── PipeTransport ────────────────→ │
       │         (内部 RTP 转发)                │
       │                                       │
       │                                       │ otherPeer.consume({ producer })
       │                                       │   → consumerRouter.canConsume()
       │                                       │   → transport.consume(...)
       │                                       │   → notify("newConsumer")
```

**关键**: `pipeToRouter` 只在 `usePipeTransports=true` 时调用。如果单 Worker，Producer 和 Consumer 在同一个 Router 内，不需要 pipe。

## 7. 错误处理和断线重连

### 7.1 Peer 超时

```typescript
// Peer.ts constructor
this.#joinTimer = setTimeout(() => {
    logger.debug(`Peer didn't join in ${JOIN_TIMEOUT_MS}ms, closing it`);
    this.close();
}, JOIN_TIMEOUT_MS); // 30 秒
```

### 7.2 WebSocket 断开

```typescript
// Peer.ts: protooPeer.on('close')
this.#protooPeer.on('close', () => {
    // 1. 清理所有 Transport/Producer/Consumer (mediasoup 侧)
    for (const transport of this.#transports.values()) {
        transport.close();       // mediasoup worker 清理
    }
    this.#producers.clear();
    this.#consumers.clear();

    // 2. 触发 disconnected 事件
    this.emit('disconnected');

    // 3. 最终 close → Room 移除 Peer
    this.close();
});
```

### 7.3 Room 侧通知其他 Peer

```typescript
// Room.ts: handlePeer()
peer.on('disconnected', () => {
    const otherPeers = this.getOtherPeers(peer);
    for (const otherPeer of otherPeers) {
        otherPeer.notify('peerClosed', { peerId: peer.id });
    }
});
```

### 7.4 重复 peerId 处理

```typescript
// Room.ts: mayCloseExistingPeer()
private mayCloseExistingPeer(peerId: PeerId): void {
    // 如果已有同 peerId 的 Peer，先关闭旧的
    if (existingPeer) { existingPeer.close(); }
    if (existingJoiningPeer) { existingJoiningPeer.close(); }
    if (existingBroadcasterPeer) { existingBroadcasterPeer.close(); }
}
```

### 7.5 Worker 崩溃处理

```typescript
// Server.ts constructor
for (const { worker } of this.#workersAndWebRtcServers.values()) {
    if (worker.closed) {
        throw new InvalidStateError(
            `mediasoup Worker is closed [pid:${worker.pid}, died:${worker.died}]`
        );
    }
}
```

### 7.6 DTLS 连接错误

```typescript
// Peer.ts: transport observer
transport.observer.on('close', () => {
    this.#transports.delete(transport.id);
});
// WebRtcTransport 的 iceConsentTimeout: 20s — 超时自动关闭
```

### 7.7 客户端断线

客户端通过 `protoo-client` 自动检测 WebSocket 断开。`RoomClient.js` 中:

```javascript
this._protoo.on('close', () => {
    if (!this._closed) {
        this.close(); // 清理所有 producer/consumer/transport
        store.dispatch(stateActions.setRoomState('closed'));
    }
});
```

## 8. 多 Worker 负载均衡

### 8.1 Round-Robin 分配

```typescript
// Server.ts
#nextWorkerIdx = 0;

private getNextWorkerAndWebRtcServer() {
    const { worker, webRtcServer } =
        this.#workersAndWebRtcServers.get(this.#nextWorkerIdx)!;

    if (++this.#nextWorkerIdx === this.#workersAndWebRtcServers.size) {
        this.#nextWorkerIdx = 0; // 轮转
    }

    return { worker, webRtcServer };
}
```

### 8.2 负载均衡策略

**分配粒度: Room 级别**

- 每个 Room 在创建时分配到**一对 Worker** (producer + consumer)
- Room 内所有 Peer 共享同一个 producerRouter 和 consumerRouter
- 新 Room → round-robin 到下一对 Worker

**设计权衡**:

| 方案 | 优点 | 缺点 |
|------|------|------|
| Room 级分配 (当前) | 实现简单，同房间媒体在单 Worker 内路由，延迟低 | 热门房间可能导致单 Worker 过载 |
| Peer 级分配 | 负载更均匀 | 同房间跨 Worker 通信开销大，需要更多 pipe |
| 分层 Worker | Producer/Consumer 分离 | 需要至少 2 个 Worker，跨 Worker pipe 有开销 |

### 8.3 usePipeTransports 的负载分布

```
usePipeTransports=false (单 Worker 模式):
  Room 内所有 peer 在同一个 Worker
  优点: 零额外转发延迟
  缺点: 单 Worker 瓶颈

usePipeTransports=true (双 Worker 模式):
  Producer → Worker 0 (producerRouter)
           ↓ pipeToRouter
  Consumer ← Worker 1 (consumerRouter)
  优点: 生产者和消费者负载分离
  缺点: 需要至少 2 Workers，pipe 有内部延迟
```

## 9. 关键技术细节

### 9.1 BWE 扩展控制

```typescript
// Room.ts constructor
if (disableBwe) {
    // 从 consumerRouter RTP capabilities 中移除传输范围 CC 扩展
    this.#consumerRouterRtpCapabilities = this.disableBweRtpExtensions();
}
```

移除的 RTP 头扩展:
- `http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01`
- `http://www.ietf.org/id/draft-holmer-rmcat-abs-send-time-13`

### 9.2 AudioLevelObserver + ActiveSpeakerObserver

```typescript
const audioLevelObserver = await producerRouter.createAudioLevelObserver({
    maxEntries: 10,
    threshold: -80,  // dB
    interval: 800,    // ms
});

const activeSpeakerObserver = await producerRouter.createActiveSpeakerObserver();
```

- Producer 创建后，audio producer 自动注册到 observer
- `audioLevelObserver.on('volumes')` → 广播给所有 peer
- `activeSpeakerObserver.on('dominantspeaker')` → 广播给所有 peer

### 9.3 Bot 功能

```typescript
const bot = await Bot.create({ usePipeTransports, producerRouter, consumerRouter });
```

Bot 在 Room 创建时自动加入，可以从文件播放音频，用于测试。

### 9.4 Network Throttle

支持通过 API 限制 Uplink/Downlink 带宽:
```typescript
// 通过 terminal client 或 API
Server.applyNetworkThrottle({ secret, options: { uplink, downlink, rtt, loss } });
```

### 9.5 BroadcasterPeer (RTP 端点)

支持非浏览器端通过 PlainTransport (RTP) 接入:
- `createPlainTransport` → `connectPlainTransport` → `produce` → `consume`
- 适用于 FFmpeg / GStreamer 等外部编码器

## 10. 消息类型总览

### protoo 请求 (client → server)

| 方法 | 说明 |
|------|------|
| `getRtcStatsUrl` | 获取 rtcstats 监控地址 |
| `getRouterRtpCapabilities` | 获取 Router 支持的 codec |
| `createWebRtcTransport` | 创建 WebRTC Transport (producer/consumer) |
| `connectWebRtcTransport` | DTLS 握手 |
| `restartIce` | ICE restart |
| `join` | 加入房间 |
| `produce` | 创建 Producer |
| `produceData` | 创建 DataProducer |
| `resume` | 恢复 Consumer |
| `pause` | 暂停 Consumer |
| `setConsumerPreferredLayers` | 设置消费层 (simulcast) |
| `consumerRequestKeyFrame` | 请求关键帧 |
| `changeDisplayName` | 改名字 |
| `chatMessage` | 聊天消息 (via DataChannel) |
| `muteMic` / `muteWebcam` | 静音/关摄像头 |
| `restartIce` | ICE 重启 |

### protoo 通知 (server → client)

| 通知 | 说明 |
|------|------|
| `mediasoupVersion` | mediasoup 版本 |
| `newPeer` | 新 peer 加入 |
| `peerClosed` | peer 离开 |
| `newConsumer` | 有新流可订阅 |
| `consumerClosed` | 流结束 |
| `consumerPaused` / `consumerResumed` | 流暂停/恢复 |
| `consumerLayersChanged` | simulcast 层变更 |
| `consumerScoreChanged` | 消费质量分变化 |
| `activeSpeaker` | 活跃发言者 |
| `speakingPeers` | 所有发言者音量 |
| `downlinkBwe` | 下行带宽估计 |
| `chatMessage` | 聊天消息 (via DataChannel) |
| `newDataConsumer` | 新 DataConsumer |

## 11. 依赖总结

| 层 | 依赖 | 版本 |
|----|------|------|
| Server | `mediasoup` | ^3.x |
| Server | `protoo-server` | (WebSocket JSON 信令) |
| Client | `mediasoup-client` | ^3.x |
| Client | `protoo-client` | (WebSocket JSON 信令) |
| Client | `awaitqueue` | (请求序列化) |
| Client | `@rtcstats/rtcstats-js` | (WebRTC 统计) |

## 参考

- [mediasoup-demo v3](https://github.com/versatica/mediasoup-demo/tree/v3)
- [mediasoup 官方文档 v3](https://mediasoup.org/documentation/v3/)
