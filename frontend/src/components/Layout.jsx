import React from 'react';
import { NavLink, useNavigate } from 'react-router-dom';
import { 
  BarChart3, 
  Wallet, 
  Settings, 
  LogOut, 
  Layers, 
  Repeat, 
  User as UserIcon,
  Bell
} from 'lucide-react';
import { useAuth } from '../context/AuthContext';

const NavItem = ({ to, icon: Icon, label }) => (
  <NavLink
    to={to}
    style={({ isActive }) => ({
      display: 'flex',
      alignItems: 'center',
      gap: '12px',
      padding: '12px 16px',
      borderRadius: '12px',
      color: isActive ? 'white' : 'var(--text-secondary)',
      background: isActive ? 'rgba(99, 102, 241, 0.1)' : 'transparent',
      textDecoration: 'none',
      fontSize: '0.95rem',
      fontWeight: isActive ? '600' : '400',
      transition: 'all 0.2s ease',
      borderLeft: isActive ? '3px solid var(--accent-color)' : '3px solid transparent'
    })}
  >
    <Icon size={20} />
    <span>{label}</span>
  </NavLink>
);

export const Layout = ({ children, title }) => {
  const { user, logout } = useAuth();
  const navigate = useNavigate();

  const handleLogout = () => {
    logout();
    navigate('/login');
  };

  return (
    <div style={{ display: 'flex', minHeight: '100vh', background: 'var(--bg-color)' }}>
      {/* Sidebar */}
      <aside style={{ 
        width: '260px', 
        borderRight: '1px solid var(--border-color)', 
        padding: '32px 16px',
        display: 'flex',
        flexDirection: 'column',
        position: 'fixed',
        height: '100vh',
        background: 'var(--bg-color)',
        zIndex: 10
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px', marginBottom: '48px', padding: '0 16px' }}>
          <div style={{ 
            width: '32px', 
            height: '32px', 
            background: 'var(--accent-color)', 
            borderRadius: '8px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontWeight: 'bold'
          }}>L</div>
          <h1 style={{ fontSize: '1.25rem', fontWeight: '700', letterSpacing: '-0.5px' }}>LeVI</h1>
        </div>

        <nav style={{ display: 'flex', flexDirection: 'column', gap: '8px', flex: 1 }}>
          <NavItem to="/" icon={BarChart3} label="Dashboard" />
          <NavItem to="/transactions" icon={Repeat} label="Transactions" />
          <NavItem to="/categories" icon={Layers} label="Categories" />
          <NavItem to="/wallets" icon={Wallet} label="Wallets" />
          <div style={{ margin: '24px 0', height: '1px', background: 'var(--border-color)' }} />
          <NavItem to="/profile" icon={UserIcon} label="Profile" />
          <NavItem to="/settings" icon={Settings} label="Settings" />
        </nav>

        <button 
          onClick={handleLogout}
          style={{ 
            display: 'flex', 
            alignItems: 'center', 
            gap: '12px', 
            padding: '12px 16px', 
            color: 'var(--danger)', 
            background: 'transparent',
            marginTop: 'auto',
            textAlign: 'left',
            width: '100%'
          }}
        >
          <LogOut size={20} />
          <span>Logout</span>
        </button>
      </aside>

      {/* Main Content */}
      <main style={{ flex: 1, marginLeft: '260px', padding: '32px 48px' }}>
        <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '40px' }}>
          <div>
            <h1 style={{ fontSize: '1.75rem', fontWeight: '700' }}>{title}</h1>
            <p style={{ color: 'var(--text-secondary)', fontSize: '0.9rem' }}>Welcome back, {user?.name || 'User'}</p>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '20px' }}>
            <button className="glass" style={{ padding: '8px', borderRadius: '10px', color: 'var(--text-secondary)' }}>
              <Bell size={20} />
            </button>
            <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
              <div style={{ textAlign: 'right' }}>
                <p style={{ fontSize: '0.9rem', fontWeight: '600' }}>{user?.name || 'User'}</p>
                <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>Premium Plan</p>
              </div>
              <div style={{ 
                width: '40px', 
                height: '40px', 
                borderRadius: '12px', 
                background: 'var(--panel-color)',
                border: '1px solid var(--border-color)',
                overflow: 'hidden'
              }}>
                <img src={`https://api.dicebear.com/7.x/avataaars/svg?seed=${user?.id}`} alt="avatar" />
              </div>
            </div>
          </div>
        </header>

        <div>{children}</div>
      </main>
    </div>
  );
};
