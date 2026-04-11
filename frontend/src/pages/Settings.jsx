import React from 'react';
import { Layout } from '../components/Layout';
import { Card, Button } from '../components/ui';
import { Bell, Moon, Globe, Lock, Eye, Database } from 'lucide-react';
import { motion } from 'framer-motion';
import api from '../api';
import { useAuth } from '../context/AuthContext';
import { useNavigate } from 'react-router-dom';

const SettingToggle = ({ icon: Icon, title, description, enabled }) => (
  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '16px 0' }}>
    <div style={{ display: 'flex', gap: '16px', alignItems: 'center' }}>
      <div style={{ padding: '10px', borderRadius: '12px', background: 'rgba(255, 255, 255, 0.05)', color: 'var(--text-secondary)' }}>
        <Icon size={20} />
      </div>
      <div>
        <p style={{ fontWeight: '600', fontSize: '1rem' }}>{title}</p>
        <p style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>{description}</p>
      </div>
    </div>
    <div style={{ 
      width: '44px', 
      height: '24px', 
      borderRadius: '12px', 
      background: enabled ? 'var(--accent-color)' : 'rgba(255, 255, 255, 0.1)',
      position: 'relative',
      cursor: 'pointer',
      transition: 'all 0.3s ease'
    }}>
      <div style={{ 
        width: '18px', 
        height: '18px', 
        borderRadius: '50%', 
        background: 'white',
        position: 'absolute',
        top: '3px',
        left: enabled ? '23px' : '3px',
        transition: 'all 0.3s ease'
      }} />
    </div>
  </div>
);

const Settings = () => {
  const { user, logout } = useAuth();
  const navigate = useNavigate();

  const handleDeleteAccount = async () => {
    if (!window.confirm('WARNING: This will permanently delete your account and all financial data. This action cannot be undone. Are you sure?')) {
      return;
    }

    try {
      await api.delete(`/users/${user.id}`);
      alert('Account deleted successfully');
      logout();
      navigate('/login');
    } catch (err) {
      console.error(err);
      alert('Failed to delete account');
    }
  };

  return (
    <Layout title="Settings">
      <div style={{ maxWidth: '800px' }}>
        <motion.div
           initial={{ opacity: 0, y: 20 }}
           animate={{ opacity: 1, y: 0 }}
           transition={{ duration: 0.5 }}
           style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}
        >
          <Card title="General Preferences">
            <div style={{ display: 'flex', flexDirection: 'column', divideY: '1px solid var(--border-color)' }}>
              <SettingToggle 
                icon={Moon} 
                title="Dark Mode" 
                description="Use the system's dark theme across the application." 
                enabled={true} 
              />
              <div style={{ height: '1px', background: 'var(--border-color)' }} />
              <SettingToggle 
                icon={Bell} 
                title="Email Notifications" 
                description="Receive weekly summaries and important budget alerts." 
                enabled={true} 
              />
              <div style={{ height: '1px', background: 'var(--border-color)' }} />
              <SettingToggle 
                icon={Globe} 
                title="Browser Push Notifications" 
                description="Stay updated with instant spending alerts." 
                enabled={false} 
              />
            </div>
          </Card>

          <Card title="Data & Privacy">
            <div style={{ display: 'flex', flexDirection: 'column' }}>
               <SettingToggle 
                icon={Eye} 
                title="Privacy Mode" 
                description="Hide account balances on the dashboard initially." 
                enabled={false} 
              />
              <div style={{ height: '1px', background: 'var(--border-color)' }} />
               <SettingToggle 
                icon={Database} 
                title="Automatic Backups" 
                description="Sync your data to our secure cloud twice a day." 
                enabled={true} 
              />
              <div style={{ height: '1px', background: 'var(--border-color)' }} />
               <SettingToggle 
                icon={Lock} 
                title="Two-Step Verification" 
                description="Add an extra layer of security to your account." 
                enabled={true} 
              />
            </div>
          </Card>

          <Card title="Danger Zone" style={{ border: '1px solid rgba(239, 68, 68, 0.2)' }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <div>
                <p style={{ fontWeight: '600', color: 'var(--danger)' }}>Delete Account</p>
                <p style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>Permanently remove your account and all data.</p>
              </div>
              <Button 
                variant="danger"
                onClick={handleDeleteAccount}
              >
                Delete Account
              </Button>
            </div>
          </Card>

          <div style={{ display: 'flex', gap: '16px', justifyContent: 'flex-end', marginTop: '16px' }}>
            <Button variant="secondary" style={{ padding: '12px 24px' }}>Reset Defaults</Button>
            <Button style={{ padding: '12px 32px' }}>Save Settings</Button>
          </div>
        </motion.div>
      </div>
    </Layout>
  );
};

export default Settings;
