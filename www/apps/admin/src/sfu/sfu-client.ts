// SFU Consumer Client — mediasoup Server-Offer transport
// Flow: CreateWebRtcTransport(recv) → WebRtcTransportCreated → buildRemoteSdp → setRemoteDescription → createAnswer → ConnectWebRtcTransport
// After consume, ontrack delivers remote stream.

// PIT-55: mediasoup consume 匹配要求完整 codec 字段 (clockRate/parameters/preferredPayloadType)，
// 缺任一 → match_codecs strict 匹配失败 → "No compatible media codecs"
// 参数与 Router/Producer 一致 (4d0032 Main, packetization-mode=1)
function videoRtpCapabilities() {
  return {
    codecs: [{
      kind: 'video', // serde(tag="kind") 必需 (PIT-55)
      mimeType: 'video/H264',
      clockRate: 90000,
      preferredPayloadType: 101,
      parameters: {
        'level-asymmetry-allowed': 1,
        'packetization-mode': 1,
        'profile-level-id': '4d0032',
      },
      rtcpFeedback: [],
    }],
    headerExtensions: [],
  };
}
interface IceParams {
  username_fragment: string;
  password: string;
}

interface DtlsParams {
  fingerprints: { algorithm: string; value: string }[];
  role: string;
}

// PIT-56: server 的 IceCandidate 是字段格式 (ip/port/protocol/foundation/priority/candidate_type)，非 SDP 字符串
interface IceCandidate {
  ip: string;
  port: number;
  protocol: string;
  foundation: string;
  priority: number;
  candidate_type?: string;
}

interface TransportCreated {
  transport_id: string;
  ice_parameters: IceParams;
  dtls_parameters: DtlsParams;
  ice_candidates?: IceCandidate[];
}

type StreamCallback = (stream: MediaStream) => void;
type StatusCallback = (status: 'connecting' | 'connected' | 'playing' | 'disconnected' | 'error') => void;
type MetricsCallback = (metrics: StreamMetrics) => void;

export interface StreamMetrics {
  rtt: number;          // ms
  packetLoss: number;   // percentage
  fps: number;
  bitrate: number;      // kbps
  jitter: number;       // ms
  resolution: string;
}

export class SfuConsumerClient {
  private ws: WebSocket | null = null;
  private closed = false;  // PIT-50: close() 后禁止重连（StrictMode 双挂载竞争）
  private pc: RTCPeerConnection | null = null;
  private onTrack: StreamCallback;
  private onStatus: StatusCallback;
  private onMetrics: MetricsCallback;
  private pendingSdp: any = null;
  private metricsTimer: ReturnType<typeof setInterval> | null = null;
  private transportId: string | null = null;
  private transportResolver: ((params: TransportCreated) => void) | null = null;
  private pendingProducer: any = null;


  constructor(
    private serverUrl: string,
    private roomId: string,
    private token: string,
    callbacks: {
      onTrack: StreamCallback;
      onStatus: StatusCallback;
      onMetrics: MetricsCallback;
    },
  ) {
    this.onTrack = callbacks.onTrack;
    this.onStatus = callbacks.onStatus;
    this.onMetrics = callbacks.onMetrics;
  }

  async connect(): Promise<void> {
    this.closed = false;  // PIT-50: 每次 connect 重置关闭标志
    this.onStatus('connecting');

    const protocol = this.serverUrl.startsWith('wss:') ? 'wss:' : 'ws:';
    const host = this.serverUrl.replace(/^wss?:\/\//, '');
    const wsUrl = `${protocol}//${host}/ws`;

    // Auth: JWT 经 sec-websocket-protocol 子协议（RFC 6455 token 禁止空格——不能带 "Bearer " 前缀）
    // PIT-49: 浏览器子协议 = 纯 JWT；server 解析时兼容 "Bearer " 前缀
    this.ws = new WebSocket(wsUrl, this.token ? [this.token] : []);

    // Auth: PSK fallback（无 token 时发明文 PSK；有 JWT 子协议则不发）
    const psk = this.token ? null : 'audemsp-dev';
    const authPromise = new Promise<void>((resolve, reject) => {
      this.ws!.onopen = () => {
        if (psk) this.ws!.send(psk);
      };
      this.ws!.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data);
          if (msg.code === 0 || msg.type === 'error' && msg.code === 0) {
            this.onStatus('connected');
            resolve();
          } else if (msg.code === 4003) {
            reject(new Error('Auth failed'));
          }
        } catch (err) {
          console.warn('SfuClient: auth message parse failed', err);
          // Non-JSON message, skip
        }
      };
      this.ws!.onerror = () => reject(new Error('WS error'));
      setTimeout(() => reject(new Error('Auth timeout')), 10000);
    });
    await authPromise;

    // Set up signaling message handler
    this.ws.onmessage = (event) => {
      this.handleMessage(event.data);
    };

    // Join room
    this.ws.send(JSON.stringify({
      type: 'room_join',
      room_id: this.roomId,
      peer_role: 'consumer',
    }));

    // Reconnect on WS close
    this.ws.onclose = () => {
      if (this.closed) return;  // PIT-50: close() 后不重连
      this.onStatus('disconnected');
      this.stopMetrics();
      this.reconnect();
    };
  }
  async startPlay(): Promise<void> {
    if (!this.ws) throw new Error('Not connected');

    // Create RTCPeerConnection upfront (shared for SFU and P2P)
    this.pc = new RTCPeerConnection({
      iceServers: [{ urls: 'stun:stun.l.google.com:19302' }],
    });
    this.pc.ontrack = (event) => { console.log('SfuClient: ONTRACK fired, streams=', event.streams.length, 'track=', event.track?.kind); this.onTrack(event.streams[0]); this.onStatus('playing'); this.startMetrics(); };
    this.pc.oniceconnectionstatechange = () => {
      console.log('SfuClient: iceConnectionState =', this.pc?.iceConnectionState); // PIT-56 观测
      if (this.pc?.iceConnectionState === 'disconnected' || this.pc?.iceConnectionState === 'failed') {
        this.onStatus('disconnected'); this.stopMetrics();
      }
    };
    this.pc.onicecandidate = (event) => {
      console.log('SfuClient: local candidate', event.candidate?.candidate); // PIT-56 观测
      if (event.candidate) this.ws?.send(JSON.stringify({ type: 'rtc_ice_candidate', room_id: this.roomId, target: null, candidate: event.candidate.candidate, sdp_mid: event.candidate.sdpMid, sdp_mline_index: event.candidate.sdpMLineIndex }));
    };
    this.pc.addTransceiver('video', { direction: 'recvonly' });
    this.pc.addTransceiver('audio', { direction: 'recvonly' });

    // Try SFU (mediasoup) with 3s timeout. Fall back to P2P if no response.
    // Set resolver BEFORE sending to avoid race condition
    const sfuPromise = new Promise<TransportCreated | null>(r => { this.transportResolver = r; });
    console.log("SfuClient: sending create_web_rtc_transport"); this.ws.send(JSON.stringify({ type: "create_web_rtc_transport", room_id: this.roomId, peer_id: this.roomId + '-consumer', direction: 'recv' }));
    const sfuResult = await Promise.race([
      sfuPromise,
      new Promise<null>(r => setTimeout(() => r(null), 3000)),
    ]);

    if (sfuResult) {
      console.log('SfuClient: SFU transport created, building SDP...');
      this.transportId = sfuResult.transport_id;
      // pending producer will be processed after connect_web_rtc_transport succeeds
      console.log('SfuClient: setting remote description...');
      const offerSdp = this.buildRemoteSdp(sfuResult.ice_parameters, sfuResult.dtls_parameters, sfuResult.ice_candidates ?? []);
      try {
        console.log('SfuClient: offer SDP:\n' + offerSdp); // PIT-56 观测
        await this.pc.setRemoteDescription({ type: 'offer', sdp: offerSdp });
        console.log('SfuClient: remote description set OK');
        const answer = await this.pc.createAnswer();
        console.log('SfuClient: answer created');
        console.log('SfuClient: answer SDP:\n' + answer.sdp); // PIT-56 观测
        await this.pc.setLocalDescription(answer);
        console.log('SfuClient: local description set, sending connect_web_rtc_transport');
        // PIT-56: connect 的 fingerprints 必须是浏览器本地证书指纹 (从 answer SDP 提取),
        // 传 sfuResult 的 (mediasoup 指纹) → DTLS fingerprint mismatch → 无 SRTP → Consumer 不转发
        const localFp = (answer.sdp ?? '').match(/a=fingerprint:(\S+) (\S+)/);
        this.ws.send(JSON.stringify({ type: 'connect_web_rtc_transport', room_id: this.roomId, peer_id: this.roomId + '-consumer', transport_id: sfuResult.transport_id, dtls_parameters: { fingerprints: localFp ? [{ algorithm: localFp[1], value: localFp[2] }] : sfuResult.dtls_parameters.fingerprints, role: "client" }, sdp: answer.sdp }));
      } catch (e) {
        console.error('SfuClient: SDP negotiation failed:', e);
      }
    } else {
      console.log("SfuClient: SFU timeout, falling back to P2P");
      if (this.pendingSdp) {
        console.log("SfuClient: replaying pending SDP");
        this.handleMessage(JSON.stringify(this.pendingSdp));
      }
    }
  }
  // WS message handler — routed from connect() onmessage
  handleMessage(data: string): void {
    try {
      const msg = JSON.parse(data);
      console.log('SfuClient: received message type:', msg.type);

      if (msg.type === 'web_rtc_transport_created' && this.transportResolver) {
        console.log('SfuClient: transport msg keys:', Object.keys(msg).join(','), 'cands=', JSON.stringify(msg.ice_candidates)); // PIT-56 观测
        console.log('SfuClient: transport created, id:', msg.transport_id);
        this.transportResolver({
          transport_id: msg.transport_id,
          ice_parameters: msg.ice_parameters,
          dtls_parameters: msg.dtls_parameters,
          ice_candidates: msg.ice_candidates ?? [], // PIT-56: 必须传给 buildRemoteSdp
        });
        this.transportResolver = null;
      } else if (msg.type === 'new_producer') {
        if (this.transportId) {
          console.log('SfuClient: consuming producer', msg.producer_id);
          const rtpCaps = videoRtpCapabilities();
          this.ws?.send(JSON.stringify({
            type: 'consume', room_id: this.roomId, transport_id: this.transportId,
            producer_id: msg.producer_id, kind: msg.kind, rtp_capabilities: rtpCaps,
          }));
        } else {
          console.log('SfuClient: new_producer before transport, queuing');
          this.pendingProducer = msg;
        }
      } else if (msg.type === 'consumed') {
        // ponytail: producer consumed, stream arrives via ontrack
      } else if (msg.type === 'error' && msg.code === 0) {
        console.log('SfuClient: transport_connected (code: 0)');
        if (this.pendingProducer && this.transportId) {
          console.log('SfuClient: consuming pending producer', this.pendingProducer.producer_id);
          // PIT-55: rtp_capabilities 需完整 codec 字段, 见 videoRtpCapabilities()
          const rtpCaps = videoRtpCapabilities();
          this.ws?.send(JSON.stringify({
            type: 'consume', room_id: this.roomId, transport_id: this.transportId,
            producer_id: this.pendingProducer.producer_id, kind: this.pendingProducer.kind, rtp_capabilities: rtpCaps,
          }));
        }
      } else if (msg.type === 'error') {
        console.log('SfuClient: error', msg.code, msg.message);
      } else if (msg.type === "sdp") {
        console.log("SfuClient: SDP received");
        if (!this.pc) { this.pendingSdp = msg; return; }
        // P2P mode: handle host's SDP offer → create answer
        try {
          const sdp = typeof msg.sdp === 'string' ? JSON.parse(msg.sdp) : msg.sdp;
          if (sdp.type === 'offer' && this.pc) {
            this.pc.setRemoteDescription(sdp).then(async () => {
              if (!this.pc) return;
              const answer = await this.pc.createAnswer();
              await this.pc.setLocalDescription(answer);
              this.ws?.send(JSON.stringify({ type: 'sdp', room_id: this.roomId, target: null, sdp: JSON.stringify(answer) }));
            }).catch((err) => console.warn('SfuClient: SDP setRemoteDescription failed', err));
          }
        } catch (err) {
          console.warn('SfuClient: SDP handling failed', err);
        }
      } else if (msg.type === 'rtc_ice_candidate' && this.pc) {
        console.log('SfuClient: ICE candidate received', msg.candidate);
        this.pc.addIceCandidate({
          candidate: msg.candidate,
          sdpMid: msg.sdp_mid ?? null,
          sdpMLineIndex: msg.sdp_mline_index ?? null,
        }).catch((e) => console.warn('SfuClient: addIceCandidate failed', e));
      }
    } catch (err) {
      console.warn('SfuClient: message handling failed', err);
    }
  }

  // Build a server-side SDP offer from mediasoup ICE/DTLS parameters.
  // The browser answers this offer to establish the server-offer transport.
  private buildRemoteSdp(ice: IceParams, dtls: DtlsParams, candidates: IceCandidate[]): string {
    const fp = dtls.fingerprints[0];
    // PIT-56: mediasoup ICE-Lite 候选必须嵌入 offer 的 m= 段（无候选 → 浏览器 ICE 无对端地址，永不发起）
    // 转为 SDP candidate 行 (candidate 必须在 m= 段内 — PIT-46 同教训)
    const toCandidateLine = (c: IceCandidate) =>
      `a=candidate:${c.foundation} 1 ${c.protocol.toUpperCase()} ${c.priority} ${c.ip} ${c.port} typ ${c.candidate_type ?? 'host'}`;
    const videoCandidates = candidates.map(toCandidateLine).join('\r\n');
    const audioCandidates = '';
    return [
      'v=0',
      'o=- 0 0 IN IP4 0.0.0.0',
      's=-',
      't=0 0',
      'a=group:BUNDLE video audio',
      'a=ice-lite',
      `a=ice-ufrag:${ice.username_fragment}`,
      `a=ice-pwd:${ice.password}`,
      `a=fingerprint:${fp.algorithm.toLowerCase()} ${fp.value}`,
      'a=setup:passive', // PIT-56: offer setup 决定 answerer 角色 — passive → 浏览器 active (ClientHello 发起方)；mediasoup 是 DTLS server 等 ClientHello (Host 侧 actpass 同理)
      // Video: H264
      'm=video 7 UDP/TLS/RTP/SAVPF 101',
      'c=IN IP4 127.0.0.1',
      'a=rtcp-mux',
      'a=mid:video',
      'a=sendonly', // PIT-56: offer 描述 mediasoup (发送方) — recvonly+浏览器recvonly → 协商 inactive → 无媒体轨
      'a=rtpmap:101 H264/90000',
      'a=fmtp:101 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f',
      ...(videoCandidates ? [videoCandidates] : []),
      'a=end-of-candidates',
      // Audio: Opus
      'm=audio 7 UDP/TLS/RTP/SAVPF 111',
      'c=IN IP4 127.0.0.1',
      'a=rtcp-mux',
      'a=mid:audio',
      'a=rtpmap:111 opus/48000/2',
      'a=fmtp:111 minptime=10;useinbandfec=1',
      ...(audioCandidates ? [audioCandidates] : []),
      'a=end-of-candidates',
      '',
    ].join('\r\n');
  }

  // startMetrics polls RTCPeerConnection.getStats() every 2s
  private startMetrics(): void {
    this.stopMetrics();
    this.metricsTimer = setInterval(async () => {
      if (!this.pc) return;
      try {
        const stats = await this.pc.getStats();
        let rtt = 0, packetsLost = 0, packetsReceived = 0, fps = 0, bitrate = 0, jitter = 0;
        let width = 0, height = 0;

        stats.forEach((report) => {
          if (report.type === 'candidate-pair' && report.state === 'succeeded') {
            rtt = Math.round((report as any).currentRoundTripTime * 1000) || 0;
          }
          if (report.type === 'inbound-rtp' && report.kind === 'video') {
            packetsLost = (report as any).packetsLost || 0;
            packetsReceived = (report as any).packetsReceived || 0;
            fps = (report as any).framesPerSecond || 0;
            bitrate = Math.round(((report as any).bytesReceived || 0) * 8 / 1000);
            jitter = Math.round(((report as any).jitter || 0) * 1000);
            width = (report as any).frameWidth || 0;
            height = (report as any).frameHeight || 0;
          }
        });

        this.onMetrics({
          rtt,
          packetLoss: packetsReceived > 0 ? Math.round((packetsLost / (packetsLost + packetsReceived)) * 10000) / 100 : 0,
          fps,
          bitrate,
          jitter,
          resolution: width && height ? `${width}x${height}` : 'unknown',
        });
      } catch (err) {
        console.warn('SfuClient: getStats failed', err);
        // getStats() may fail; non-critical
      }
    }, 2000);
  }

  private stopMetrics(): void {
    if (this.metricsTimer) {
      clearInterval(this.metricsTimer);
      this.metricsTimer = null;
    }
  }

  private async reconnect(): Promise<void> {
    const maxRetries = 5;
    let delay = 1000;
    
    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      console.log(`SfuClient: reconnecting (attempt ${attempt}/${maxRetries})...`);
      await new Promise(r => setTimeout(r, delay));
      
      try {
        await this.connect();
        console.log('SfuClient: reconnected successfully');
        return;
      } catch (err) {
        console.warn(`SfuClient: reconnect attempt ${attempt} failed`, err);
        delay = Math.min(delay * 2, 30000);
      }
    }
    
    console.error('SfuClient: max reconnect attempts reached');
    this.onStatus('error');
  }

  close(): void {
    this.closed = true;  // PIT-50: 先设标志防 onclose 重连
    this.stopMetrics();
    this.pc?.close();
    this.pc = null;
    this.ws?.close();
    this.ws = null;
    this.transportId = null;
    this.transportResolver = null;
  }
}
