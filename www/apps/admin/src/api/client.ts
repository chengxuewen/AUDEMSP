const BASE = '/api/admin';
const LOGIN_URL = '/api/auth/login';
const TOKEN_KEY = 'mediaservo_admin_token';

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

function headers(): Record<string, string> {
  const token = getToken();
  return token ? { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' } : { 'Content-Type': 'application/json' };
}

async function request<T>(path: string, opts?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { ...opts, headers: { ...headers(), ...opts?.headers } });
  if (res.status === 401) {
    // 401 自愈: 仅对失效/过期 token 清登录态跳转（valid-but-denied 如 operator 保留错误态 — roles-operator.spec）
    const token = getToken();
    if (!token || isTokenExpired(token)) {
      clearToken();
      const base = window.location.pathname.startsWith('/admin') ? '/admin' : '';
      window.location.href = `${base}/login`;
    }
    throw new Error('Authentication required — please sign in');
  }
  return res.json();
}

/** JWT exp（秒）是否已过期；无 exp/解析失败视为失效。 */
function isTokenExpired(token: string): boolean {
  const claims = parseToken(token);
  return !claims?.exp || claims.exp * 1000 <= Date.now();
}

// ── JWT claims + auth 状态（H3: dispatcher 角色感知渲染）──────────────────────

export interface JwtClaims {
  sub?: string;
  role?: string;
  vehicles?: string[];
  iat?: number;
  exp?: number;
}

/** base64url 解码 JWT payload（无库依赖; 结构异常 → null）。 */
export function parseToken(token: string): JwtClaims | null {
  try {
    const payload = token.split('.')[1];
    if (!payload) return null;
    return JSON.parse(atob(payload.replace(/-/g, '+').replace(/_/g, '/')));
  } catch {
    return null;
  }
}

export function getRole(): string | null {
  const token = getToken();
  return token ? (parseToken(token)?.role ?? null) : null;
}

export function getUsername(): string | null {
  const token = getToken();
  return token ? (parseToken(token)?.sub ?? null) : null;
}

/** 同标签页内 token 变更通知（login/logout 后 Layout/nav 重渲染）。 */
const authListeners = new Set<() => void>();
function notifyAuth() { authListeners.forEach((fn) => fn()); }
export function subscribeAuth(fn: () => void): () => void {
  authListeners.add(fn);
  return () => { authListeners.delete(fn); };
}

// Types
export interface Consumer { peer_id: string; connected_since: string; }
export interface StreamSnapshot { stream_id: string; consumers: Consumer[]; }
export interface DeviceSnapshot { device_id: string; online_since: string; streams: StreamSnapshot[]; }
export interface DeviceListResponse { devices: DeviceSnapshot[]; total_devices: number; }
export interface StatsResponse { active_rooms: number; connected_peers: number; uptime_seconds: number; }

// H3: SFU 房间摘要（音频会议面板数据源）。
export interface SfuRoom {
  room_id: string;
  participants: number;
  producers: number;
  consumers: number;
  audio: boolean;
  producer_ids: string[];
  consumer_ids: string[];
}
export interface SfuRoomsResponse { rooms: SfuRoom[]; }

// H3: SfuStats（镜像 WS 信令 SfuStats — H2 协议的管理面）。
export interface SfuStats {
  producer_id?: string;
  consumer_id?: string;
  kind?: 'audio' | 'video';
  byte_count: number;
  packet_count: number;
  score: number;
}

// H3: 多车状态上报（StatusReport wire 镜像 — E3）。
export interface TopicFlow { topic: string; fps: number; bps: number; last_ts_mono_ns: number; frames: number; stalled: boolean; }
export interface StreamFlow { id: string; bytes_sent: number; frames_encoded: number; frame_width: number; frame_height: number; connected: boolean; }
export interface ProcessState { name: string; running: boolean; expected: boolean; }
export interface ChildSignal { src: string; connected: boolean; last_msg_secs: number; }
export interface SignalStatus {
  remote_connected: boolean;
  remote_since_secs?: number;
  remote_peer_id: string;
  children: ChildSignal[];
  agent_uptime_secs: number;
}
export interface StatusReport {
  room_id: string;
  topics: TopicFlow[];
  streams: StreamFlow[];
  processes: ProcessState[];
  signal: SignalStatus;
  ts: number;
  config_version: number;
}
export interface VehicleStatusResponse { vehicles: { room_id: string; report: StatusReport }[]; }

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

export async function getSfuRooms(): Promise<SfuRoomsResponse> {
  return request('/sfu/rooms');
}

export async function getSfuStats(producerId?: string, consumerId?: string): Promise<SfuStats> {
  const params = new URLSearchParams();
  if (producerId) params.set('producer_id', producerId);
  if (consumerId) params.set('consumer_id', consumerId);
  return request(`/sfu/stats?${params.toString()}`);
}

export async function getVehicleStatus(): Promise<VehicleStatusResponse> {
  return request('/status');
}

export interface LoginResponse { token: string; username: string; role: string; expires_in_secs: number; }

export async function login(username: string, password: string): Promise<LoginResponse> {
  const res = await fetch(LOGIN_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  });
  if (!res.ok) throw new Error(res.status === 401 ? 'Invalid username or password' : `Login failed: ${res.status}`);
  return res.json();
}

export function connectEvents(): WebSocket {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const token = getToken();
  const url = `${protocol}//${window.location.host}/api/admin/events`;
  // ponytail: pass token via query param for WS (no custom headers in browser WebSocket)
  return new WebSocket(token ? `${url}?token=${encodeURIComponent(token)}` : url);
}

export function setToken(token: string) { localStorage.setItem(TOKEN_KEY, token); notifyAuth(); }
export function clearToken() { localStorage.removeItem(TOKEN_KEY); notifyAuth(); }
export function hasToken(): boolean { return !!getToken(); }
