// SFU Consumer Client — WebSocket + RTCPeerConnection for mediasoup transport
// Phase 1: P2P mode (direct WS signaling relay to Host)
// Phase 2: SFU mode (mediasoup CreateWebRtcTransport → Connect → Consume)

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

// ponytail: Phase 1 uses P2P signaling (SDP relay through WS).
// Phase 2+ will switch to mediasoup transport (CreateWebRtcTransport).
// The SfuConsumerClient class abstracts both modes.

export class SfuConsumerClient {
  private ws: WebSocket | null = null;
  private pc: RTCPeerConnection | null = null;
  private onTrack: StreamCallback;
  private onStatus: StatusCallback;
  private onMetrics: MetricsCallback;
  private metricsTimer: ReturnType<typeof setInterval> | null = null;

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

    // Phase 1: P2P — connect WS, auth, join room, setup peer connection
    const protocol = this.serverUrl.startsWith('wss:') ? 'wss:' : 'ws:';
    const host = this.serverUrl.replace(/^wss?:\/\//, '');
    const wsUrl = `${protocol}//${host}/ws`;

    this.ws = new WebSocket(wsUrl);
    this.ws.onerror = () => this.onStatus('error');

    await new Promise<void>((resolve, reject) => {
      this.ws!.onopen = () => resolve();
      this.ws!.onerror = () => reject(new Error('WS connect failed'));
      setTimeout(() => reject(new Error('WS timeout')), 10000);
    });

    // Auth
    const authMsg = JSON.stringify({ secret: this.token || 'omspbase-dev' });
    this.ws.send(authMsg);
    this.onStatus('connected');

    // Join room
    this.ws.send(JSON.stringify({
      type: 'room_join',
      room_id: this.roomId,
      peer_role: 'consumer',
    }));
  }

  async startPlay(): Promise<void> {
    if (!this.ws) throw new Error('Not connected');

    // Create RTCPeerConnection
    this.pc = new RTCPeerConnection({
      iceServers: [
        { urls: 'stun:stun.l.google.com:19302' },
      ],
    });

    // Handle incoming tracks
    this.pc.ontrack = (event) => {
      this.onTrack(event.streams[0]);
      this.onStatus('playing');
      this.startMetrics();
    };

    // Handle ICE candidates
    this.pc.onicecandidate = (event) => {
      if (event.candidate && this.ws) {
        this.ws.send(JSON.stringify({
          type: 'rtc_ice_candidate',
          room_id: this.roomId,
          target: null,
          candidate: event.candidate.candidate,
          sdp_mid: event.candidate.sdpMid,
          sdp_mline_index: event.candidate.sdpMLineIndex,
        }));
      }
    };

    // Handle ICE connection state changes
    this.pc.oniceconnectionstatechange = () => {
      const state = this.pc?.iceConnectionState;
      if (state === 'disconnected' || state === 'failed') {
        this.onStatus('disconnected');
        this.stopMetrics();
      }
    };

    // Add recv-only transceivers
    this.pc.addTransceiver('video', { direction: 'recvonly' });
    this.pc.addTransceiver('audio', { direction: 'recvonly' });

    // Create SDP offer
    const offer = await this.pc.createOffer();
    await this.pc.setLocalDescription(offer);

    // Send offer to server (relay to host)
    this.ws.send(JSON.stringify({
      type: 'sdp',
      room_id: this.roomId,
      target: null,
      sdp: JSON.stringify(offer),
    }));
  }

  // WS message handler (call from parent component)
  handleMessage(data: string): void {
    try {
      const msg = JSON.parse(data);
      if (msg.type === 'sdp' && this.pc) {
        const sdp = typeof msg.sdp === 'string' ? msg.sdp : JSON.stringify(msg.sdp);
        const desc = JSON.parse(sdp);
        this.pc.setRemoteDescription(desc).catch(() => {});
      } else if (msg.type === 'rtc_ice_candidate' && this.pc) {
        this.pc.addIceCandidate({
          candidate: msg.candidate,
          sdpMid: msg.sdp_mid,
          sdpMLineIndex: msg.sdp_mline_index,
        }).catch(() => {});
      }
    } catch {
      // Ignore parse errors for non-signaling messages
    }
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
  }
}
