import { useState, type FormEvent } from 'react';
import { Navigate, useNavigate } from 'react-router-dom';
import { login, setToken, hasToken } from '../api/client';
import './Login.css';

/** 登录页 — 复用 Settings 登录表单样式（.login-input/.btn/.token-status）。 */
export default function Login() {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loggingIn, setLoggingIn] = useState(false);
  const navigate = useNavigate();

  if (hasToken()) return <Navigate to="/" replace />;

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!username.trim() || !password) return;
    setLoggingIn(true);
    setError(null);
    try {
      const resp = await login(username.trim(), password);
      setToken(resp.token);
      // 角色感知默认视图: operator 无 admin REST 准入（I1）→ 落 Settings 而非触发 Dashboard 401
      navigate(resp.role === 'admin' || resp.role === 'dispatcher' ? '/' : '/settings');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Login failed');
    } finally {
      setLoggingIn(false);
    }
  };

  return (
    <div className="login-page">
      <form className="login-card" onSubmit={handleSubmit}>
        <h1 className="login-title">MediaServo Admin</h1>
        <p className="login-subtitle">Sign in with a cockpit account</p>
        <input
          type="text"
          className="login-input"
          placeholder="username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="username"
          autoFocus
        />
        <input
          type="password"
          className="login-input"
          placeholder="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="current-password"
        />
        {error && <p className="token-status error">{error}</p>}
        <button type="submit" className="btn" disabled={loggingIn}>
          {loggingIn ? 'Signing in...' : 'Login'}
        </button>
      </form>
    </div>
  );
}
