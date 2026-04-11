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
  Bell,
  Moon,
  Sun
} from 'lucide-react';
import { useAuth } from '../context/AuthContext';
import { useTheme } from '../context/ThemeContext';

const NavItem = ({ to, icon: Icon, label }) => (
  <NavLink
    to={to}
    style={({ isActive }) => ({
      display: 'flex',
      alignItems: 'center',
      gap: '12px',
      padding: '12px 16px',
      borderRadius: '12px',
      color: isActive ? 'var(--bg-color)' : 'var(--text-secondary)',
      background: isActive ? 'var(--text-primary)' : 'transparent',
      textDecoration: 'none',
      fontSize: '0.9rem',
      fontWeight: '500',
      transition: 'all 0.2s ease',
      margin: '0 8px'
    })}
  >
    <Icon size={20} />
    <span>{label}</span>
  </NavLink>
);

export const Layout = ({ children, title }) => {
  const { user, logout } = useAuth();
  const { theme, toggleTheme } = useTheme();
  const navigate = useNavigate();

  const handleLogout = () => {
    logout();
    navigate('/login');
  };

  return (
    <div style={{ display: 'flex', minHeight: '100vh', background: 'var(--bg-color)' }}>
      {/* Sidebar */}
      <aside style={{ 
        width: '240px', 
        borderRight: '1px solid var(--border-color)', 
        padding: '32px 0',
        display: 'flex',
        flexDirection: 'column',
        position: 'fixed',
        height: '100vh',
        background: 'var(--sidebar-bg)',
        zIndex: 10
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px', marginBottom: '48px', padding: '0 16px' }}>
          <div style={{ 
            width: '32px', 
            height: '32px', 
            background: 'var(--text-primary)', 
            color: 'var(--bg-color)',
            borderRadius: '8px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontWeight: 'bold',
            boxShadow: '0 2px 10px rgba(255,255,255,0.1)'
          }}>L</div>
          <h1 style={{ fontSize: '1.25rem', fontWeight: '700', letterSpacing: '-0.5px' }}>LeVI</h1>
        </div>

        <nav style={{ display: 'flex', flexDirection: 'column', gap: '8px', flex: 1 }}>
          <NavItem to="/dashboard" icon={BarChart3} label="Dashboard" />
          <NavItem to="/transactions" icon={Repeat} label="Transactions" />
          <NavItem to="/categories" icon={Layers} label="Categories" />
          <NavItem to="/wallets" icon={Wallet} label="Wallets" />
          <div style={{ margin: '24px 0', height: '1px', background: 'var(--border-color)' }} />
          <NavItem to="/profile" icon={UserIcon} label="Profile" />
          <NavItem to="/settings" icon={Settings} label="Settings" />
        </nav>

        <div style={{ marginTop: 'auto', display: 'flex', flexDirection: 'column', gap: '8px' }}>
          <button 
            onClick={toggleTheme}
            style={{ 
              display: 'flex', 
              alignItems: 'center', 
              gap: '12px', 
              padding: '12px 16px', 
              color: 'var(--text-secondary)', 
              background: 'transparent',
              textAlign: 'left',
              width: '100%',
              border: 'none',
              cursor: 'pointer'
            }}
          >
            {theme === 'dark' ? <Sun size={20} /> : <Moon size={20} />}
            <span>{theme === 'dark' ? 'Light Mode' : 'Dark Mode'}</span>
          </button>
          
          <button 
            onClick={handleLogout}
            style={{ 
              display: 'flex', 
              alignItems: 'center', 
              gap: '12px', 
              padding: '12px 16px', 
              color: 'var(--danger)', 
              background: 'transparent',
              textAlign: 'left',
              width: '100%',
              border: 'none',
              cursor: 'pointer'
            }}
          >
            <LogOut size={20} />
            <span>Logout</span>
          </button>
        </div>
      </aside>

      {/* Main Content */}
      <main style={{ flex: 1, marginLeft: '240px', padding: '32px 40px' }}>
        <div>{children}</div>
      </main>
    </div>
  );
};
