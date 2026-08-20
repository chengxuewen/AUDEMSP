import { NavLink, Outlet, useNavigate } from 'react-router-dom';
import { useAuth } from '../hooks/useAuth';
import { clearToken } from '../api/client';
import './Layout.css';

export default function Layout() {
  const { role, username, canMonitor, token } = useAuth();
  const navigate = useNavigate();

  const handleLogout = () => {
    clearToken();
    navigate('/login');
  };
  return (
    <div className="layout">
      <header className="header">
        <span className="logo">📡 MediaServo Admin</span>
        <div className="header-right">
          <span className="version">
            {username ? `${username}${role ? ` [${role}]` : ''} · ` : ''}v0.1.0
          </span>
          {token && (
            <button className="logout-btn" onClick={handleLogout}>Logout</button>
          )}
        </div>
      </header>
      <div className="main">
        <nav className="sidebar">
          <NavLink to="/" end className={({ isActive }) => isActive ? 'nav-item active' : 'nav-item'}>
            📊 Dashboard
          </NavLink>
          {/* H3: 音频会议 + 多车监控 = G3 can_status 角色（operator/admin/dispatcher） */}
          {canMonitor && (
            <NavLink to="/audio" className={({ isActive }) => isActive ? 'nav-item active' : 'nav-item'}>
              🎙️ Audio Conference
            </NavLink>
          )}
          {canMonitor && (
            <NavLink to="/vehicles" className={({ isActive }) => isActive ? 'nav-item active' : 'nav-item'}>
              🚗 Vehicles
            </NavLink>
          )}
          <NavLink to="/settings" className={({ isActive }) => isActive ? 'nav-item active' : 'nav-item'}>
            ⚙️ Settings
          </NavLink>
        </nav>
        <main className="content">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
