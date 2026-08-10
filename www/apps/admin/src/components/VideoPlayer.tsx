import { useRef, useEffect, useState } from 'react';
import { SfuConsumerClient, type StreamMetrics } from '../sfu/sfu-client';
import './VideoPlayer.css';

interface Props {
  roomId: string;
  serverUrl: string;
  token: string;
  onClose: () => void;
}

type ConnectionStatus = 'connecting' | 'connected' | 'playing' | 'disconnected' | 'error';

const STATUS_COLORS: Record<ConnectionStatus, string> = {
  connecting: '#f39c12',
  connected: '#27ae60',
  playing: '#27ae60',
  disconnected: '#e74c3c',
  error: '#e74c3c',
};

export default function VideoPlayer({ roomId, serverUrl, token, onClose }: Props) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const clientRef = useRef<SfuConsumerClient | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>('connecting');
  const [metrics, setMetrics] = useState<StreamMetrics | null>(null);
  const [showStats, setShowStats] = useState(false);
  const [showControls, setShowControls] = useState(false);
  let controlsTimer: ReturnType<typeof setTimeout>;

  useEffect(() => {
    // PIT-76: 首帧渲染时间观测
    const t0 = performance.now();
    const logT = (msg: string) => console.log(`[T+${Math.round(performance.now() - t0)}ms] [VideoPlayer] ${msg}`);
    const client = new SfuConsumerClient(serverUrl, roomId, token, {
      onTrack: (stream) => {
        logT('onTrack 收到 stream');
        if (videoRef.current) {
          videoRef.current.srcObject = stream;
          videoRef.current.play().catch(() => {});
          // 首帧检测: loadedmetadata 触发时 videoWidth > 0
          videoRef.current.onloadedmetadata = () => logT('video loadedmetadata (videoWidth=' + videoRef.current?.videoWidth + ')');
          videoRef.current.onplaying = () => logT('video onplaying');
          // 轮询 videoWidth 确认首帧实际渲染
          const poll = setInterval(() => {
            const v = videoRef.current;
            if (v && v.videoWidth > 0) {
              const now = performance.now();
              const playT0 = (window as any).__playT0 as number | undefined;
              const total = playT0 ? now - playT0 : null;
              logT('首帧渲染确认 videoWidth=' + v.videoWidth + 'x' + v.videoHeight);
              console.log(`[Play→首帧] 总耗时: ${total !== null ? Math.round(total) + 'ms' : 'N/A (无 __playT0)'} (点击Play→首帧渲染)`);
              clearInterval(poll);
            }
          }, 100);
          setTimeout(() => clearInterval(poll), 30000); // 30s 上限
        }
      },
      onStatus: (s) => { logT('status = ' + s); setStatus(s); },
      onMetrics: setMetrics,
    });

    clientRef.current = client;
    logT('connect() 调用');
    client.connect().then(() => client.startPlay()).catch(() => setStatus('error'));

    return () => {
      client.close();
      clientRef.current = null;
    };
  }, [roomId, serverUrl, token]);

  const handleMouseMove = () => {
    setShowControls(true);
    clearTimeout(controlsTimer);
    controlsTimer = setTimeout(() => setShowControls(false), 2000);
  };

  const statusColor = STATUS_COLORS[status];
  const isDisconnected = status === 'disconnected' || status === 'error';

  return (
    <div className="video-player-overlay" onClick={onClose}>
      <div
        className={`video-player ${isDisconnected ? 'disconnected' : ''}`}
        onClick={(e) => e.stopPropagation()}
        onMouseMove={handleMouseMove}
        onMouseLeave={() => setShowControls(false)}
      >
        {/* Top bar */}
        <div className="vp-top-bar" style={{ background: isDisconnected ? '#c0392b' : '#2c3e50' }}>
          <span className="vp-status-dot" style={{ background: statusColor }} />
          <span className="vp-title">{roomId}</span>
          {metrics && !isDisconnected && (
            <span className="vp-bitrate">{metrics.resolution} · {Math.round(metrics.bitrate / 1000)}Mbps</span>
          )}
          <button className="vp-close" onClick={onClose}>✕</button>
        </div>

        {/* Video */}
        <div className="vp-body">
          <video ref={videoRef} autoPlay playsInline muted />
          {status === 'connecting' && <div className="vp-status-msg">Connecting...</div>}
          {isDisconnected && <div className="vp-status-msg error">Signal Lost</div>}
        </div>

        {/* Controls (hover) */}
        {showControls && !isDisconnected && (
          <div className="vp-controls">
            <button title="Mute">🔇</button>
            <button title="Pause">⏸</button>
            <button title="Fullscreen" onClick={() => videoRef.current?.requestFullscreen()}>⛶</button>
          </div>
        )}

        {/* Metrics bar */}
        {metrics && !isDisconnected && (
          <div className="vp-metrics-bar" onClick={() => setShowStats(!showStats)}>
            <span>⚡{metrics.rtt}ms</span>
            <span>📦{metrics.packetLoss}%</span>
            <span>🎬{metrics.fps}fps</span>
          </div>
        )}
        {isDisconnected && (
          <div className="vp-metrics-bar disconnected">
            <span>⚠️ Connection Lost</span>
          </div>
        )}

        {/* Detail stats panel */}
        {showStats && metrics && (
          <div className="vp-stats-panel" onClick={(e) => e.stopPropagation()}>
            <h4>Stream Stats</h4>
            <div className="stats-grid">
              <div><label>RTT</label><span>{metrics.rtt}ms</span></div>
              <div><label>Jitter</label><span>{metrics.jitter}ms</span></div>
              <div><label>Packet Loss</label><span>{metrics.packetLoss}%</span></div>
              <div><label>Bitrate</label><span>{Math.round(metrics.bitrate / 1000)}Mbps</span></div>
              <div><label>FPS</label><span>{metrics.fps}</span></div>
              <div><label>Resolution</label><span>{metrics.resolution}</span></div>
            </div>
            <button className="vp-stats-close" onClick={() => setShowStats(false)}>✕</button>
          </div>
        )}
      </div>
    </div>
  );
}
