import { useState } from 'react';
import { setToken, clearToken, hasToken, login, getRole, getUsername } from '../api/client';
import './Settings.css';

export default function Settings() {
  const [username, setUsernameInput] = useState('');
  const [password, setPassword] = useState('');
  const [token, setTokenInput] = useState('');
  const [saved, setSaved] = useState(hasToken());
  const [loginError, setLoginError] = useState<string | null>(null);
  const [loggingIn, setLoggingIn] = useState(false);
  const currentUser = getUsername();
  const currentRole = getRole();

  const handleLogin = async () => {
    if (!username.trim() || !password) return;
    setLoggingIn(true);
    setLoginError(null);
    try {
      const resp = await login(username.trim(), password);
      setToken(resp.token);
      setUsernameInput('');
      setPassword('');
      setSaved(true);
    } catch (e) {
      setLoginError(e instanceof Error ? e.message : 'Login failed');
    } finally {
      setLoggingIn(false);
    }
  };

  const handleSave = () => {
    if (token.trim()) {
      setToken(token.trim());
      setTokenInput('');
      setSaved(true);
    }
  };

  const handleClear = () => {
    clearToken();
    setSaved(false);
  };

  return (
    <div className="settings">
      <h2>Settings</h2>

      <section className="setting-group">
        <h3>Account Login</h3>
        <p className="setting-desc">Sign in with a cockpit account (G3 accounts.yaml — viewer/operator/admin/dispatcher). The token is stored locally; role-aware views apply.</p>
        {currentUser && (
          <p className="token-status saved">✅ Signed in as {currentUser}{currentRole ? ` [${currentRole}]` : ''}</p>
        )}
        <div className="token-row">
          <input
            type="text"
            className="token-input"
            placeholder="username"
            value={username}
            onChange={(e) => setUsernameInput(e.target.value)}
          />
          <input
            type="password"
            className="token-input"
            placeholder="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          <button className="btn" onClick={handleLogin} disabled={loggingIn}>
            {loggingIn ? 'Signing in...' : 'Login'}
          </button>
        </div>
        {loginError && <p className="token-status error">{loginError}</p>}
      </section>

      <section className="setting-group">
        <h3>Admin Token</h3>
        <p className="setting-desc">Or paste the admin JWT token from the server startup output, or generate one with <code>--create-admin-token</code>.</p>
        <div className="token-row">
          <input
            type="text"
            className="token-input"
            placeholder="eyJhbGciOiJIUzI1NiIs..."
            value={token}
            onChange={(e) => setTokenInput(e.target.value)}
          />
          <button className="btn" onClick={handleSave}>Save Token</button>
          {saved && <button className="btn btn-outline" onClick={handleClear}>Clear</button>}
        </div>
        {saved && !currentUser && <p className="token-status saved">✅ Token saved</p>}
      </section>

      <section className="setting-group">
        <h3>About</h3>
        <p>MediaServo Admin Dashboard v0.1.0</p>
        <p className="setting-desc">Remote control scenario — monitor device streams, consumers, audio conference rooms, and vehicle status.</p>
      </section>
    </div>
  );
}
