interface StatsCardProps {
  label: string;
  value: string | number;
  unit?: string;
}

export default function StatsCard({ label, value, unit }: StatsCardProps) {
  return (
    <div className="stats-card">
      <span className="stats-label">{label}</span>
      <span className="stats-value">{value}<small>{unit ? ` ${unit}` : ''}</small></span>
    </div>
  );
}
