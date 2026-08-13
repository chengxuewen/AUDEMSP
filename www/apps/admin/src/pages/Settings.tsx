import { useState } from 'react';
import { setToken, clearToken, hasToken } from '../api/client';
import './Settings.css';

export default function Settings() {
  const [token, setTokenInput] = useState('');
  const [saved, setSaved] = useState(hasToken());

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
        <h3>Admin Authentication</h3>
        <p className="setting-desc">Paste the admin JWT token from the server startup output, or generate one with <code>--create-admin-token</code>.</p>
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
        {saved && <p className="token-status saved">✅ Token saved</p>}
      </section>

      <section className="setting-group">
        <h3>About</h3>
        <p>MediaServo Admin Dashboard v0.1.0</p>
        <p className="setting-desc">Remote control scenario — monitor device streams, consumers, and server health.</p>
      </section>
    </div>
  );
}
