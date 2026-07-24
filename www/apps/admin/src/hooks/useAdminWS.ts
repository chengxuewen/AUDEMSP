import { useEffect, useRef } from 'react';
import { connectEvents, hasToken } from '../api/client';

export function useAdminWS(onEvent: (event: any) => void) {
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    if (!hasToken()) return;

    const connect = () => {
      const ws = connectEvents();
      wsRef.current = ws;
      ws.onmessage = (e) => {
        try { onEvent(JSON.parse(e.data)); } catch { /* ignore malformed */ }
      };
      ws.onclose = () => {
        // ponytail: reconnect after 5s
        setTimeout(connect, 5000);
      };
    };

    connect();
    return () => wsRef.current?.close();
  }, [onEvent]);
}
