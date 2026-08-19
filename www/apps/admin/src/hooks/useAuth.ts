import { useEffect, useState } from 'react';
import { getRole, getToken, getUsername, subscribeAuth } from '../api/client';

/** H3: 认证状态 hook — token/role/username 随 setToken/clearToken 变更（同标签页）。 */
export function useAuth() {
  const [token, setTokenState] = useState<string | null>(getToken());
  const [role, setRole] = useState<string | null>(getRole());
  const [username, setUsername] = useState<string | null>(getUsername());

  useEffect(() => {
    return subscribeAuth(() => {
      setTokenState(getToken());
      setRole(getRole());
      setUsername(getUsername());
    });
  }, []);

  const isAdmin = role === 'admin';
  const isDispatcher = role === 'dispatcher';
  // I1 review 修复: 与 server auth_middleware 准入对齐（admin|dispatcher）—
  // G3 can_status 含 operator，但 admin REST 只放行 admin/dispatcher；
  // operator 显 nav 会全 401 → 前端收紧，operator 回落默认视图。
  const canMonitor = isAdmin || isDispatcher;

  return { token, role, username, isAdmin, isDispatcher, canMonitor };
}
