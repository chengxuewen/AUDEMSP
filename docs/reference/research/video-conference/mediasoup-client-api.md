# mediasoup-client v3 TypeScript API Reference

> **Source**: [versatica/mediasoup-client v3](https://github.com/versatica/mediasoup-client)
> **Generated**: 2026-07-31 | Extracted from upstream source: Device.ts, Transport.ts, Consumer.ts, Producer.ts

---

## 1. Architecture Overview

mediasoup-client is a **non-SDP** WebRTC client. It replaces the standard SDP offer/answer
exchange with a plain-JSON signaling protocol. Key design decisions:

- **Server-offer only**: The mediasoup server creates `WebRtcTransport`, generates all
  ICE/DTLS parameters, and sends them to the client. The client never generates SDP.
- **No SDP anywhere**: `iceParameters`, `iceCandidates`, `dtlsParameters` are passed as
  plain JSON objects.
- **Transport = ICE+DTLS pipe**: A `Transport` connects the client to a mediasoup Router.
  Send transports carry outbound media; recv transports carry inbound media.
- **Producer/Consumer = RTP streams**: `Producer` wraps an outbound `MediaStreamTrack`;
  `Consumer` wraps an inbound `MediaStreamTrack`.

```
Browser                          mediasoup Server
───────                          ────────────────
Device.load(routerRtpCapabilities)
sendTransport.produce(track)  →  Producer on Router
recvTransport.consume({...})  ←  Consumer on Router
```

---

## 2. Core Classes

### 2.1 Device

The entry point. Holds codec capabilities after `load()`.

```ts
import { Device } from 'mediasoup-client';

const device = new Device();
```

#### `device.load({ routerRtpCapabilities, preferLocalCodecsOrder? })`

Computes the intersection of browser-native codecs and the server's router capabilities.
Must be called before creating any transport.

```ts
const routerRtpCapabilities = await signaling.request('getRouterCapabilities');

await device.load({ routerRtpCapabilities });
// optionally: preferLocalCodecsOrder: true — prioritize local codec order over router order
```

After `load()`:
- `device.loaded: boolean`
- `device.recvRtpCapabilities: RtpCapabilities` — what this device can receive
- `device.sendRtpCapabilities: RtpCapabilities` — what this device can send
- `device.canProduce('video' | 'audio'): boolean`
- `device.sctpCapabilities: SctpCapabilities | undefined`

#### `device.createSendTransport(options)` / `device.createRecvTransport(options)`

Both accept the same `TransportOptions`:

```ts
interface TransportOptions<AppData> {
  id: string;                         // server-assigned transport ID
  iceParameters: IceParameters;       // from server WebRtcTransport
  iceCandidates: IceCandidate[];      // from server WebRtcTransport
  dtlsParameters: DtlsParameters;     // from server WebRtcTransport
  sctpParameters?: SctpParameters;    // from server (if DataChannel needed)
  iceServers?: RTCIceServer[];        // optional STUN/TURN
  iceTransportPolicy?: RTCIceTransportPolicy;
  additionalSettings?: any;
  appData?: AppData;
}
```

**Important**: `id`, `iceParameters`, `iceCandidates`, `dtlsParameters` all come from
the server's `webRtcTransport` — the client never fabricates them.

---

## 3. Transport Lifecycle

### 3.1 Creating a Transport (server-offer flow)

```
1. Client: signaling.request('createWebRtcTransport', { ... })
2. Server: router.createWebRtcTransport({ ... })
3. Server: returns { id, iceParameters, iceCandidates, dtlsParameters, sctpParameters }
4. Client: device.createSendTransport(serverParams)
5. Transport emits 'connect' → client signals dtlsParameters to server
6. Server: transport.connect({ dtlsParameters })
7. Transport: connectionState → 'connected'
```

```ts
// Step 4: Create local transport from server parameters
const sendTransport = device.createSendTransport({
  id: serverParams.id,
  iceParameters: serverParams.iceParameters,
  iceCandidates: serverParams.iceCandidates,
  dtlsParameters: serverParams.dtlsParameters,
  sctpParameters: serverParams.sctpParameters,
});
```

### 3.2 Mandatory Event Handlers

Two handlers are **required** before `produce()` or `consume()` will work:

#### `transport.on('connect', callback)`

Called when the transport needs to perform DTLS handshake. The client must signal
its local `dtlsParameters` to the server, then call `callback()`.

```ts
sendTransport.on('connect', async ({ dtlsParameters }, callback, errback) => {
  try {
    await signaling.request('connectWebRtcTransport', {
      transportId: sendTransport.id,
      dtlsParameters,
    });
    callback();  // tells transport DTLS is done server-side
  } catch (error) {
    errback(error);
  }
});
```

#### `transport.on('produce', callback)` — send transports only

Called when the local side needs to create a server-side Producer. Client signals
`kind` + `rtpParameters` to server, server returns `{ id }`, client calls `callback({ id })`.

```ts
sendTransport.on('produce', async ({ kind, rtpParameters, appData }, callback, errback) => {
  try {
    const { id } = await signaling.request('produce', {
      transportId: sendTransport.id,
      kind,
      rtpParameters,
      appData,
    });
    callback({ id });
  } catch (error) {
    errback(error);
  }
});
```

### 3.3 State Properties

| Property | Type | Values |
|----------|------|--------|
| `transport.connectionState` | `ConnectionState` | `'new'` → `'connecting'` → `'connected'` → `'disconnected'` → `'failed'` → `'closed'` |
| `transport.iceGatheringState` | `IceGatheringState` | `'new'` → `'gathering'` → `'complete'` |
| `transport.id` | `string` | Server-assigned transport ID |
| `transport.closed` | `boolean` | |
| `transport.appData` | `AppData` | Mutable custom data |

State change listener:

```ts
sendTransport.on('connectionstatechange', (connectionState) => {
  console.log('Transport connection state:', connectionState);
});
```

---

## 4. Producing Media (Send Transport)

### 4.1 `transport.produce(options)`

```ts
async produce<AppData>({
  track,                    // MediaStreamTrack (required)
  encodings?,               // RtpEncodingParameters[] for simulcast
  codecOptions?,            // { videoGoogleStartBitrate?, videoGoogleMaxBitrate?, ... }
  codec?,                   // Force specific codec
  stopTracks = true,        // Stop track when Producer closes
  disableTrackOnPause = true,
  zeroRtpOnPause = false,
  onRtpSender?,             // (rtpSender: RTCRtpSender) => void
  appData = {},             // Custom data for signaling
}): Promise<Producer>
```

**Prerequisites**:
1. Transport `connectionState` must not be `'new'` unless `'connect'` listener is set
2. `'produce'` listener must be set on the transport
3. Track must not be `ended`
4. `device.canProduce(track.kind)` must be true

**What happens internally**:
1. `handler.send({ track, encodings, codecOptions, codec })` → gets local `rtpParameters`
2. Emits `'produce'` event with `{ kind, rtpParameters, appData }`
3. App signals to server, gets back `{ id }`
4. Creates `Producer` instance with server `id`

**Minimal example**:

```ts
const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
const videoTrack = stream.getVideoTracks()[0];

const producer = await sendTransport.produce({ track: videoTrack });
console.log('Producer ID:', producer.id);
// producer.track === videoTrack
```

### 4.2 Producer

| Property/Method | Description |
|----------------|-------------|
| `producer.id` | Server-assigned Producer ID |
| `producer.kind` | `'audio'` or `'video'` |
| `producer.track` | `MediaStreamTrack` |
| `producer.paused` | `boolean` |
| `producer.appData` | Mutable custom data |
| `producer.close()` | Close producer (stops track if `stopTracks=true`) |
| `producer.pause()` | Pause sending |
| `producer.resume()` | Resume sending |
| `producer.getStats()` | `Promise<RTCStatsReport>` |

**Events**:
- `producer.on('transportclose')` — transport closed
- `producer.on('trackended')` — track ended
- `producer.observer.on('close' | 'pause' | 'resume' | 'trackended')`

---

## 5. Consuming Media (Recv Transport)

### 5.1 `transport.consume(options)`

```ts
async consume<AppData>({
  id,                  // Consumer ID from server
  producerId,          // Producer ID from server
  kind,                // 'audio' | 'video'
  rtpParameters,       // RTP parameters from server
  streamId?,           // Optional stream identifier
  onRtpReceiver?,      // (rtpReceiver: RTCRtpReceiver) => void
  appData = {},
}): Promise<Consumer>
```

**Prerequisites**:
1. Transport must be receiving direction (`device.createRecvTransport()`)
2. `'connect'` listener must be set (unless already connected)
3. `device` must be capable of receiving the codec (checked internally via `ortc.canReceive()`)

**How to get the parameters**: The server signals a `newConsumer` event with
`{ id, producerId, kind, rtpParameters }`. The client passes these directly to `consume()`.

```ts
// Server sends over signaling:
// { consumerId: '...', producerId: '...', kind: 'video', rtpParameters: {...} }

const consumer = await recvTransport.consume({
  id: serverData.consumerId,
  producerId: serverData.producerId,
  kind: serverData.kind,
  rtpParameters: serverData.rtpParameters,
});

// consumer.track is a MediaStreamTrack — attach it to a video element
videoElement.srcObject = new MediaStream([consumer.track]);
```

### 5.2 Consumer

| Property/Method | Description |
|----------------|-------------|
| `consumer.id` | Server-assigned Consumer ID |
| `consumer.producerId` | Associated Producer ID |
| `consumer.kind` | `'audio'` or `'video'` |
| `consumer.track` | `MediaStreamTrack` — attach to `<video>` / `<audio>` |
| `consumer.paused` | `boolean` |
| `consumer.appData` | Mutable custom data |
| `consumer.close()` | Close consumer |
| `consumer.pause()` | Pause receiving — sets `track.enabled = false` |
| `consumer.resume()` | Resume receiving — sets `track.enabled = true` |
| `consumer.getStats()` | `Promise<RTCStatsReport>` |

**Events**:
- `consumer.on('transportclose')` — transport closed
- `consumer.on('trackended')` — track ended
- `consumer.observer.on('close' | 'pause' | 'resume' | 'trackended')`

---

## 6. Signaling Protocol

### 6.1 Client → Server Messages

| Message | Direction | Payload |
|---------|-----------|---------|
| `getRouterCapabilities` | C→S | (none) |
| `createWebRtcTransport` | C→S | `{ sctpCapabilities?, forceTcp?, ... }` |
| `connectWebRtcTransport` | C→S | `{ transportId, dtlsParameters }` |
| `produce` | C→S | `{ transportId, kind, rtpParameters, appData? }` |
| `consume` | C→S | `{ transportId, producerId, rtpCapabilities }` |
| `resumeConsumer` | C→S | `{ consumerId }` |
| `pauseConsumer` | C→S | `{ consumerId }` |

### 6.2 Server → Client Messages

| Message | Direction | Payload |
|---------|-----------|---------|
| `routerCapabilities` | S→C | `{ rtpCapabilities }` |
| `webRtcTransportCreated` | S→C | `{ id, iceParameters, iceCandidates, dtlsParameters, sctpParameters? }` |
| `transportConnected` | S→C | `{ transportId }` |
| `producerCreated` | S→C | `{ id }` |
| `consumerCreated` | S→C | `{ id, producerId, kind, rtpParameters }` |
| `newProducer` | S→C | `{ producerId }` — broadcast to other peers |

> **Note**: mediasoup does not mandate a specific protocol framing. These messages can
> be delivered over WebSocket, HTTP long-poll, or any bidirectional channel. The client
> library itself only provides the event/callback pattern — the application layer
> implements the transport.

### 6.3 Interaction Sequence (Single Peer Producing)

```
Client A (Producer)                  Server                      Client B (Consumer)
─────────────────                    ──────                      ─────────────────
  |                                    |                              |
  |── getRouterCapabilities ────────→  |                              |
  |←── routerCapabilities ───────────|                              |
  |                                    |                              |
  |── createWebRtcTransport ────────→  |                              |
  |←── webRtcTransportCreated ───────|                              |
  |    { id, iceParameters, ... }      |                              |
  |                                    |                              |
  | createSendTransport(params)        |                              |
  |                                    |                              |
  |── connectWebRtcTransport ───────→  |                              |
  |    { transportId, dtlsParameters }  |                              |
  |                                    | transport.connect(params)     |
  |←── transportConnected ───────────|                              |
  |                                    |                              |
  | getUserMedia() → track             |                              |
  | transport.produce({ track })       |                              |
  |    ↓ internal handler.send()       |                              |
  |    ↓ 'produce' event fires         |                              |
  |                                    |                              |
  |── produce ─────────────────────→  |                              |
  |    { transportId, kind,            |                              |
  |      rtpParameters }               |                              |
  |                                    | router.produce(params)       |
  |←── producerCreated ──────────────|                              |
  |    { id }                          |                              |
  |    ↓ callback({ id })              |                              |
  | Producer ready                     |                              |
  |                                    |                              |
  |                                    |  ── newProducer ─────────→  |
  |                                    |    { producerId }            |
  |                                    |                              |
  |                                    |  ←── consume ────────────── |
  |                                    |    { transportId,            |
  |                                    |      producerId, rtpCap }    |
  |                                    |                              |
  |                                    |  ── consumerCreated ──────→  |
  |                                    |    { id, producerId, kind,   |
  |                                    |      rtpParameters }         |
  |                                    |                              |
  |                                    |            consumer =        |
  |                                    |      transport.consume({...})|
  |                                    |            video.srcObject = |
  |                                    |      [consumer.track]        |
  |                                    |                              |
  |═══════════════ RTP media flowing ════════════════════════════════|
```

---

## 7. ICE / DTLS State Machine

### 7.1 Transport states

```
connectionState:
  'new' ──→ 'connecting' ──→ 'connected'
     │                          │
     │                          ├──→ 'disconnected' (ICE disconnected, may recover)
     │                          │
     └──→ 'failed'              └──→ 'failed'
                                     │
                                     └──→ 'closed' (transport.close())
```

### 7.2 ICE gathering states

```
iceGatheringState:
  'new' ──→ 'gathering' ──→ 'complete'
```

### 7.3 Monitoring

```ts
transport.on('connectionstatechange', (state) => {
  switch (state) {
    case 'connected':
      console.log('ICE+DTLS established');
      break;
    case 'disconnected':
      console.warn('ICE disconnected — may recover');
      break;
    case 'failed':
      console.error('Transport failed — recreate');
      break;
  }
});

transport.on('icegatheringstatechange', (state) => {
  if (state === 'complete') console.log('ICE gathering done');
});
```

---

## 8. Minimal Runnable Example

```ts
import { Device } from 'mediasoup-client';

// --- Initialization ---
const device = new Device();

// Fetch router capabilities from server
const routerRtpCapabilities = await fetch('/api/router-capabilities').then(r => r.json());
await device.load({ routerRtpCapabilities });

if (!device.canProduce('video')) {
  throw new Error('Browser cannot produce video');
}

// --- Send Transport ---
const sendParams = await fetch('/api/create-send-transport').then(r => r.json());
const sendTransport = device.createSendTransport(sendParams);

sendTransport.on('connect', async ({ dtlsParameters }, callback, errback) => {
  try {
    await fetch('/api/connect-transport', {
      method: 'POST',
      body: JSON.stringify({ transportId: sendTransport.id, dtlsParameters }),
    });
    callback();
  } catch (e) { errback(e as Error); }
});

sendTransport.on('produce', async ({ kind, rtpParameters }, callback, errback) => {
  try {
    const { id } = await fetch('/api/produce', {
      method: 'POST',
      body: JSON.stringify({ transportId: sendTransport.id, kind, rtpParameters }),
    }).then(r => r.json());
    callback({ id });
  } catch (e) { errback(e as Error); }
});

// --- Publish Media ---
const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
const videoProducer = await sendTransport.produce({ track: stream.getVideoTracks()[0] });
const audioProducer = await sendTransport.produce({ track: stream.getAudioTracks()[0] });

console.log('Producing:', videoProducer.id, audioProducer.id);

// --- Recv Transport ---
const recvParams = await fetch('/api/create-recv-transport').then(r => r.json());
const recvTransport = device.createRecvTransport(recvParams);

recvTransport.on('connect', async ({ dtlsParameters }, callback, errback) => {
  try {
    await fetch('/api/connect-transport', {
      method: 'POST',
      body: JSON.stringify({ transportId: recvTransport.id, dtlsParameters }),
    });
    callback();
  } catch (e) { errback(e as Error); }
});

// --- Subscribe to Remote Producer ---
// (triggered by server signaling, e.g. WebSocket)
async function onNewProducer(producerInfo: {
  consumerId: string; producerId: string; kind: string; rtpParameters: any;
}) {
  const consumer = await recvTransport.consume({
    id: producerInfo.consumerId,
    producerId: producerInfo.producerId,
    kind: producerInfo.kind as 'audio' | 'video',
    rtpParameters: producerInfo.rtpParameters,
  });

  const element = consumer.kind === 'video'
    ? document.querySelector<HTMLVideoElement>('#remote-video')!
    : document.querySelector<HTMLAudioElement>('#remote-audio')!;

  element.srcObject = new MediaStream([consumer.track]);

  // Pause/Resume controls
  document.querySelector('#pause-btn')!.addEventListener('click', () => consumer.pause());
  document.querySelector('#resume-btn')!.addEventListener('click', () => consumer.resume());
}
```

---

## 9. Key Differences from Standard WebRTC

| Standard WebRTC | mediasoup-client |
|----------------|-----------------|
| SDP offer/answer exchange | Plain JSON (`iceParameters`, `dtlsParameters`) |
| `new RTCPeerConnection()` | `device.createSendTransport()` |
| `pc.addTrack(track)` | `transport.produce({ track })` |
| `pc.ontrack` event | `transport.consume({ ... })` returns Consumer |
| `pc.iceConnectionState` | `transport.connectionState` |
| Client or server can create offer | Always server-offer (server creates WebRtcTransport) |
| One PeerConnection for all streams | Separate send + recv transports |

---

## 10. Reference

- [mediasoup-client GitHub](https://github.com/versatica/mediasoup-client)
- [mediasoup.org documentation](https://mediasoup.org/documentation/v3/)
- [mediasoup Server API](https://mediasoup.org/documentation/v3/mediasoup/api/)
