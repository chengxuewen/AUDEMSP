# Conference SDK API 参考

## 1. 概述

Conference SDK 提供创建和管理视频会议的完整 API。通过 napi-rs 绑定暴露给 TypeScript/JavaScript，同时提供 Rust crate 原生接口。

### 安装

```bash
# npm (through 宿主平台)
npm install @mediaservo/conference-sdk

# Rust (through 第三方平台)
cargo add mediaservo-conference
```

### 初始化

```typescript
import { ConferenceClient } from "@mediaservo/conference-sdk";

const client = new ConferenceClient({
  endpoint: "wss://media.example.com",
  token: "jwt-token-here",
});
```

## 2. ConferenceClient

### 2.1 房间管理

```typescript
// 创建房间
const room = await client.rooms.create({
  name: "Team Standup",
  maxParticipants: 16,
  mode: "conference",
  videoCodec: "vp9",
  simulcast: true,
  svc: true,
});
// → Room { id, name, status, createdAt, ... }

// 加入房间
const joinResult = await client.rooms.join(room.id);
// → JoinResult { room, routerRtpCapabilities }

// 离开房间
await client.rooms.leave(room.id);

// 结束房间
await client.rooms.end(room.id);

// 获取房间信息
const roomInfo = await client.rooms.get(room.id);

// 列出房间
const rooms = await client.rooms.list({ status: "active" });
```

### 2.2 媒体发布

```typescript
// 创建发送 Transport
const sendTransport = await client.transports.create({
  roomId: room.id,
  direction: "send",
  iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
});

// 发布视频
const videoProducer = await sendTransport.produce({
  track: localVideoTrack,     // MediaStreamTrack
  encodings: [
    { rid: "r0", maxBitrate: 150_000, scaleResolutionDownBy: 4 },
    { rid: "r1", maxBitrate: 400_000, scaleResolutionDownBy: 2 },
    { rid: "r2", maxBitrate: 1_200_000, scaleResolutionDownBy: 1 },
  ],
  codecOptions: { videoGoogleStartBitrate: 1000 },
});
// → Producer { id, kind, stats }

// 发布音频
const audioProducer = await sendTransport.produce({
  track: localAudioTrack,
});

// 暂停/恢复发布
await videoProducer.pause();
await videoProducer.resume();

// 关闭发布
await videoProducer.close();
```

### 2.3 媒体订阅

```typescript
// 创建接收 Transport
const recvTransport = await client.transports.create({
  roomId: room.id,
  direction: "recv",
});

// 订阅特定音视频
const consumer = await recvTransport.consume({
  producerId: videoProducer.id,  // 远程 participant 的 producer ID
  rtpCapabilities: routerRtpCapabilities,
});
// → Consumer { id, track, kind, type (simulcast/svc), producerPaused }

// 将 remote track 挂载到 <video> 元素
videoElement.srcObject = new MediaStream([consumer.track]);

// 切换编码层 (Simulcast)
await consumer.setPreferredLayers({ rid: "r1" });

// 切换编码层 (SVC)
await consumer.setPreferredLayers({
  spatialLayer: 1,
  temporalLayer: 2,
});

// 暂停/恢复订阅
await consumer.pause();
await consumer.resume();

// 关闭订阅
await consumer.close();
```

### 2.4 参与者管理

```typescript
// 获取参与者列表
const participants = await client.participants.list(room.id);

// 静音参与者 (Host only)
await client.participants.mute({ roomId: room.id, participantId: "id", kind: "audio" });

// 取消静音
await client.participants.unmute({ roomId: room.id, participantId: "id", kind: "audio" });

// 移除参与者
await client.participants.kick({ roomId: room.id, participantId: "id" });

// 设置角色
await client.participants.setRole({
  roomId: room.id,
  participantId: "id",
  role: "speaker",
});
```

### 2.5 录制

```typescript
// 开始录制
const recording = await client.recording.start({
  roomId: room.id,
  mode: "individual",  // "individual" | "composite"
  layout: "grid",      // composite 布局
});
// → RecordingStatus { id, startedAt, format }

// 停止录制
await client.recording.stop(room.id);

// 查询状态
const status = await client.recording.status(room.id);
```

## 3. 事件监听

```typescript
// 参与者加入
client.on("participant.joined", (event) => {
  console.log(`${event.participant.displayName} joined`);
});

// 参与者离开
client.on("participant.left", (event) => {
  console.log(`${event.participantId} left: ${event.reason}`);
});

// 新音轨加入 (自动订阅)
client.on("track.added", async (event) => {
  const consumer = await recvTransport.consume({
    producerId: event.track.producerId,
    rtpCapabilities: routerRtpCapabilities,
  });
  // 挂载到 UI
});

// 音轨移除
client.on("track.removed", (event) => {
  // 清理 UI
});

// 活跃发言人切换
client.on("dominant.speaker.changed", (event) => {
  // 高亮当前发言人
});

// 连接状态
client.on("connection.state", (event) => {
  console.log(`State: ${event.state}`);
});

// 错误
client.on("error", (event) => {
  console.error(`Error: ${event.code} - ${event.message}`);
});
```

## 4. Rust API

```rust
// Crate: mediaservo-conference
// 通过 napi-rs 绑定暴露给 Node.js，同时提供原生 Rust API

use mediaservo_conference::{ConferenceClient, ConferenceConfig, Room};

#[tokio::main]
async fn main() -> Result<()> {
    let client = ConferenceClient::new(ConferenceConfig {
        endpoint: "wss://media.example.com".into(),
        token: std::env::var("MEDIASERVO_TOKEN")?,
    });

    let room = client.create_room(CreateRoomParams {
        name: "Team Standup".into(),
        max_participants: 16,
        mode: RoomMode::Conference,
        video_codec: VideoCodec::Vp9,
        simulcast: true,
        svc: true,
    }).await?;

    println!("Room created: {}", room.id);
    Ok(())
}
```

## 5. 配置

```typescript
interface ConferenceConfig {
  // 必须
  endpoint: string;           // 信令端点
  token: string;              // 认证 Token

  // 可选
  iceServers?: RTCIceServer[];   // ICE 服务器
  videoCodec?: string;        // 默认视频编码
  audioCodec?: string;        // 默认音频编码
  simulcast?: boolean;        // Simulcast
  svc?: boolean;              // SVC
  maxBitrate?: number;        // 最大码率 (kbps)
  maxResolution?: string;     // 最大分辨率

  // 调试
  logLevel?: "error" | "warn" | "info" | "debug";
  statsInterval?: number;     // 统计上报间隔 (ms)
}
```

## 6. 错误处理

```typescript
import { ConferenceError } from "@mediaservo/conference-sdk";

try {
  await client.rooms.join("non-existent-room");
} catch (error) {
  if (error instanceof ConferenceError) {
    switch (error.code) {
      case "ROOM_NOT_FOUND":
        // 房间不存在
        break;
      case "ROOM_FULL":
        // 房间已满
        break;
      case "FORBIDDEN":
        // 无权限
        break;
      default:
        // 其他错误
    }
  }
}
```