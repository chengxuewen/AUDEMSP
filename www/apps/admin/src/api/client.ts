const BASE = '/api/admin';
const TOKEN_KEY = 'mediaservo_admin_token';

function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

function headers(): Record<string, string> {
  const token = getToken();
  return token ? { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' } : { 'Content-Type': 'application/json' };
}

async function request<T>(path: string, opts?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { ...opts, headers: { ...headers(), ...opts?.headers } });
  if (res.status === 401) throw new Error('Authentication required — set admin token in Settings');
  if (!res.ok) throw new Error(`API error: ${res.status}`);
  return res.json();
}

// Types
export interface Consumer { peer_id: string; connected_since: string; }
export interface StreamSnapshot { stream_id: string; consumers: Consumer[]; }
export interface DeviceSnapshot { device_id: string; online_since: string; streams: StreamSnapshot[]; }
export interface DeviceListResponse { devices: DeviceSnapshot[]; total_devices: number; }
export interface StatsResponse { active_rooms: number; connected_peers: number; uptime_seconds: number; }

// API functions
export async function getDevices(): Promise<DeviceListResponse> {
  return request('/rooms');
}

export async function getStats(): Promise<StatsResponse> {
  return request('/stats');
}

export async function deleteRoom(roomId: string): Promise<void> {
  await request(`/rooms/${roomId}`, { method: 'DELETE' });
}

export function connectEvents(): WebSocket {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const token = getToken();
  const url = `${protocol}//${window.location.host}/api/admin/events`;
  // ponytail: pass token via query param for WS (no custom headers in browser WebSocket)
  return new WebSocket(token ? `${url}?token=${encodeURIComponent(token)}` : url);
}

export function setToken(token: string) { localStorage.setItem(TOKEN_KEY, token); }
export function clearToken() { localStorage.removeItem(TOKEN_KEY); }
export function hasToken(): boolean { return !!getToken(); }
