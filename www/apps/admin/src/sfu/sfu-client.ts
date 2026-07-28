// SFU Consumer Client — mediasoup Server-Offer transport
// Flow: CreateWebRtcTransport(recv) → WebRtcTransportCreated → buildRemoteSdp → setRemoteDescription → createAnswer → ConnectWebRtcTransport
// After consume, ontrack delivers remote stream.

interface IceParams {
  username_fragment: string;
  password: string;
}

interface DtlsParams {
  fingerprints: { algorithm: string; value: string }[];
  role: string;
}

interface TransportCreated {
  transport_id: string;
  ice_parameters: IceParams;
  dtls_parameters: DtlsParams;
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
  private pc: RTCPeerConnection | null = null;
  private onTrack: StreamCallback;
  private onStatus: StatusCallback;
  private onMetrics: MetricsCallback;
  private pendingSdp: any = null;
  private metricsTimer: ReturnType<typeof setInterval> | null = null;
  private transportId: string | null = null;
  private transportResolver: ((params: TransportCreated) => void) | null = null;

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
    this.onStatus('connecting');

    const protocol = this.serverUrl.startsWith('wss:') ? 'wss:' : 'ws:';
    const host = this.serverUrl.replace(/^wss?:\/\//, '');
    const wsUrl = `${protocol}//${host}/ws`;

    this.ws = new WebSocket(wsUrl);

    // Auth: send PSK as raw string (not JSON)
    const psk = this.token || 'omspbase-dev';
    const authPromise = new Promise<void>((resolve, reject) => {
      this.ws!.onopen = () => {
        this.ws!.send(psk);
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
        } catch {
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
  }

  async startPlay(): Promise<void> {
    if (!this.ws) throw new Error('Not connected');

    // Create RTCPeerConnection upfront (shared for SFU and P2P)
    this.pc = new RTCPeerConnection({
      iceServers: [{ urls: 'stun:stun.l.google.com:19302' }],
    });
    this.pc.ontrack = (event) => { this.onTrack(event.streams[0]); this.onStatus('playing'); this.startMetrics(); };
    this.pc.oniceconnectionstatechange = () => {
      if (this.pc?.iceConnectionState === 'disconnected' || this.pc?.iceConnectionState === 'failed') {
        this.onStatus('disconnected'); this.stopMetrics();
      }
    };
    this.pc.onicecandidate = (event) => {
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
      this.transportId = sfuResult.transport_id;
      const offerSdp = this.buildRemoteSdp(sfuResult.ice_parameters, sfuResult.dtls_parameters);
      await this.pc.setRemoteDescription({ type: 'offer', sdp: offerSdp });
      const answer = await this.pc.createAnswer();
      await this.pc.setLocalDescription(answer);
      this.ws.send(JSON.stringify({ type: 'connect_web_rtc_transport', room_id: this.roomId, peer_id: this.roomId + '-consumer', transport_id: sfuResult.transport_id, dtls_parameters: { fingerprints: sfuResult.dtls_parameters.fingerprints, role: "client" }, sdp: answer.sdp }));
    }
    console.log("SfuClient: SFU timeout, falling back to P2P"); if (this.pendingSdp) { console.log("SfuClient: replaying pending SDP"); this.handleMessage(JSON.stringify(this.pendingSdp)); }
  }

  // WS message handler — routed from connect() onmessage
  handleMessage(data: string): void {
    try {
      const msg = JSON.parse(data);

      if (msg.type === 'web_rtc_transport_created' && this.transportResolver) {
        this.transportResolver({
          transport_id: msg.transport_id,
          ice_parameters: msg.ice_parameters,
          dtls_parameters: msg.dtls_parameters,
        });
        this.transportResolver = null;
      } else if (msg.type === 'new_producer' && this.transportId) {
        this.ws?.send(JSON.stringify({
          type: 'consume',
          room_id: this.roomId,
          transport_id: this.transportId,
          producer_id: msg.producer_id,
          kind: msg.kind,
        }));
      } else if (msg.type === 'consumed') {
        // ponytail: producer consumed, stream will arrive via ontrack
      } else if (msg.type === "sdp") { console.log("SfuClient: SDP received"); if (!this.pc) { this.pendingSdp = msg; return; }
        // P2P mode: handle host's SDP offer → create answer
        try {
          const sdp = typeof msg.sdp === 'string' ? JSON.parse(msg.sdp) : msg.sdp;
        if (sdp.type === 'offer' && this.pc) {
          this.pc.setRemoteDescription(sdp).then(async () => {
            if (!this.pc) return;
            const answer = await this.pc.createAnswer();
            await this.pc.setLocalDescription(answer);
              this.ws?.send(JSON.stringify({
                type: 'sdp', room_id: this.roomId, target: null,
                sdp: JSON.stringify(answer),
              }));
            }).catch(() => {});
          }
        } catch {}
      }
      else if (msg.type === 'rtc_ice_candidate' && this.pc) {
        console.log('SfuClient: ICE candidate received', msg.candidate);
        this.pc.addIceCandidate({
          candidate: msg.candidate,
          sdpMid: msg.sdp_mid ?? null,
          sdpMLineIndex: msg.sdp_mline_index ?? null,
        }).catch((e) => console.warn('SfuClient: addIceCandidate failed', e));
      }
    } catch {
      // ponytail: malformed messages are non-critical
    }
  }

  // Build a server-side SDP offer from mediasoup ICE/DTLS parameters.
  // The browser answers this offer to establish the server-offer transport.
  private buildRemoteSdp(ice: IceParams, dtls: DtlsParams): string {
    const fp = dtls.fingerprints[0];
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
      'a=setup:active',
      'm=video 7 UDP/TLS/RTP/SAVPF 100',
      'c=IN IP4 127.0.0.1',
      'a=rtcp-mux',
      'a=mid:0',
      'a=sendonly',
      'a=rtpmap:100 VP8/90000',
      'm=audio 0 UDP/TLS/RTP/SAVPF 0',
      'a=mid:1',
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
      } catch {
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

  close(): void {
    this.stopMetrics();
    this.pc?.close();
    this.pc = null;
    this.ws?.close();
    this.ws = null;
    this.transportId = null;
    this.transportResolver = null;
  }
}
