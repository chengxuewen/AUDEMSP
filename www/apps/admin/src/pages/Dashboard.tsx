import { useDevices } from '../hooks/useDevices';
import { useAdminWS } from '../hooks/useAdminWS';
import StatsCard from '../components/StatsCard';
import StatusBadge from '../components/StatusBadge';
import { getStats, deleteRoom } from '../api/client';
import { useState, useEffect, useCallback } from 'react';
import type { StatsResponse, DeviceSnapshot, StreamSnapshot } from '../api/client';
import StreamDetail from '../components/StreamDetail';
import VideoPlayer from '../components/VideoPlayer';
import './Dashboard.css';

export default function Dashboard() {
  const { devices, loading, error } = useDevices();
  const [stats, setStats] = useState<StatsResponse | null>(null);
  const [expanded, setExpanded] = useState(new Set());
  const [selectedStream, setSelectedStream] = useState<{ deviceId: string; stream: StreamSnapshot } | null>(null);
  const [playingRoom, setPlayingRoom] = useState<string | null>(null);

  const fetchStats = useCallback(async () => {
    try { setStats(await getStats()); } catch { /* ignore */ }
  }, []);

  useEffect(() => { fetchStats(); }, [fetchStats]);

  useAdminWS(() => {
    fetchStats();
    // ponytail: full refetch on any event; incremental merge when complexity warrants it
  });

  const toggle = (id: string) => {
    setExpanded(prev => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  };

  const totalStreams = devices.reduce((s, d) => s + d.streams.length, 0);
  const totalConsumers = devices.reduce((s, d) => s + d.streams.reduce((c, st) => c + st.consumers.length, 0), 0);

  if (loading) return <div className="loading">Loading...</div>;
  if (error) return <div className="error">{error}</div>;

  return (
    <div className="dashboard">
      <div className="stats-bar">
        <StatsCard label="Devices" value={devices.length} />
        <StatsCard label="Streams" value={totalStreams} />
        <StatsCard label="Consumers" value={totalConsumers} />
        <StatsCard label="Uptime" value={stats ? Math.floor(stats.uptime_seconds / 3600) : '-'} unit="h" />
      </div>

      <h2 className="section-title">Active Devices</h2>
      {devices.length === 0 ? (
        <p className="empty">No active devices</p>
      ) : (
        <div className="device-list">
          {devices.map((device) => (
            <DeviceGroup key={device.device_id} device={device} expanded={expanded.has(device.device_id)} onToggle={() => toggle(device.device_id)} onSelectStream={(stream) => setSelectedStream({ deviceId: device.device_id, stream })} onPlayStream={() => setPlayingRoom(device.device_id)} />
          ))}
        </div>
      )}
      {selectedStream && (
        <StreamDetail
          deviceId={selectedStream.deviceId}
          streamId={selectedStream.stream.stream_id}
          consumers={selectedStream.stream.consumers}
          onClose={() => setSelectedStream(null)}
        />
      )}
      {playingRoom && (
        <VideoPlayer
          roomId={playingRoom}
          serverUrl={`ws://${window.location.host}`}
          token={localStorage.getItem('audemsp_admin_token') || ''}
          onClose={() => setPlayingRoom(null)}
        />
      )}
    </div>
  );
}

function DeviceGroup({ device, expanded, onToggle, onSelectStream, onPlayStream }: { device: DeviceSnapshot; expanded: boolean; onToggle: () => void; onSelectStream: (stream: StreamSnapshot) => void; onPlayStream: () => void }) {
  const status = device.streams.length > 0 ? 'online' : 'offline';

  return (
    <div className="device-group">
      <div className="device-header" onClick={onToggle}>
        <span className={`tree-icon ${expanded ? 'expanded' : ''}`}>▶</span>
        <span className="device-name">{device.device_id}</span>
        <StatusBadge status={status} />
        <span className="device-stream-count">{device.streams.length} streams</span>
        <button className="btn-play" onClick={(e) => { e.stopPropagation(); onPlayStream(); }}>▶ Play</button>
      </div>
      {expanded && (
        <div className="stream-list">
          {device.streams.map((stream) => (
            <div key={stream.stream_id} className="stream-item" onClick={() => onSelectStream(stream)} style={{ cursor: 'pointer' }}>
              <span className="stream-name">📹 {stream.stream_id}</span>
              <span className="consumer-count">{stream.consumers.length} viewers</span>
              <div className="consumer-list">
                {stream.consumers.map((c) => (
                  <span key={c.peer_id} className="consumer-tag">👁 {c.peer_id}</span>
                ))}
              </div>
              <button className="btn-sm" onClick={(e) => { e.stopPropagation(); deleteRoom(`${device.device_id}_${stream.stream_id}`); }}>Close</button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
