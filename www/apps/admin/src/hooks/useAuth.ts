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
  // G3 矩阵: can_status = operator/admin/dispatcher（音频 + 状态视图同矩阵）。
  const canMonitor = role === 'operator' || isAdmin || isDispatcher;

  return { token, role, username, isAdmin, isDispatcher, canMonitor };
}
