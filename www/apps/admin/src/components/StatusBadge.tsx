interface StatusBadgeProps {
  status: 'online' | 'offline' | 'degraded';
}

const colors: Record<string, string> = { online: '#27ae60', offline: '#e74c3c', degraded: '#f39c12' };

export default function StatusBadge({ status }: StatusBadgeProps) {
  return (
    <span className="status-badge" style={{ color: colors[status] }}>
      <span className="status-dot" style={{ background: colors[status] }} />
      {status}
    </span>
  );
}
