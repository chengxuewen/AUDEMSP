import { useState, useEffect, useCallback } from 'react';
import { getDevices, type DeviceSnapshot } from '../api/client';

export function useDevices() {
  const [devices, setDevices] = useState<DeviceSnapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetch = useCallback(async () => {
    try {
      const data = await getDevices();
      setDevices(data.devices);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to fetch devices');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { fetch(); }, [fetch]);

  // ponytail: simple polling; WS events merge in useAdminWS
  useEffect(() => {
    const interval = setInterval(fetch, 5000);
    return () => clearInterval(interval);
  }, [fetch]);

  return { devices, loading, error, refetch: fetch };
}
