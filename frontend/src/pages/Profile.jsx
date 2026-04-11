import React from 'react';
import { Layout } from '../components/Layout';
import { Card, Button, Input } from '../components/ui';
import { useAuth } from '../context/AuthContext';
import { User, Mail, Shield, ShieldCheck, CreditCard } from 'lucide-react';
import { motion } from 'framer-motion';
import api from '../api';

const Profile = () => {
  const { user, setUser } = useAuth(); // Assuming setUser exists or we can update local state
  const [profile, setProfile] = React.useState({
    name: user?.name || '',
    email: user?.email || '',
    image: user?.image || ''
  });
  const [loading, setLoading] = React.useState(false);

  const handleUpdateProfile = async () => {
    try {
      setLoading(true);
      const res = await api.put(`/users/${user.id}`, profile);
      // If we have a setUser in context, use it. Otherwise, alert.
      if (setUser) setUser(res.data);
      alert('Profile updated successfully');
    } catch (err) {
      console.error(err);
      alert('Failed to update profile');
    } finally {
      setLoading(false);
    }
  };

  return (
    <Layout title="Profile">
      <div style={{ maxWidth: '800px' }}>
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5 }}
        >
          <div style={{ display: 'flex', gap: '32px', marginBottom: '32px', alignItems: 'center' }}>
            <div style={{ 
              width: '120px', 
              height: '120px', 
              borderRadius: '50%', 
              background: 'var(--panel-color)',
              border: '2px solid var(--border-color)',
              overflow: 'hidden'
            }}>
              <img src={`https://api.dicebear.com/7.x/avataaars/svg?seed=${user?.id}`} alt="avatar" style={{ width: '100%', height: '100%' }} />
            </div>
            <div>
              <h2 style={{ fontSize: '2rem', fontWeight: '700', marginBottom: '8px' }}>{user?.name || 'User'}</h2>
              <div style={{ display: 'flex', gap: '16px' }}>
                <span style={{ padding: '4px 12px', borderRadius: '20px', background: 'rgba(99, 102, 241, 0.1)', color: 'var(--accent-color)', fontSize: '0.8rem', fontWeight: '600', display: 'flex', alignItems: 'center', gap: '6px' }}>
                  <ShieldCheck size={14} /> Verified Member
                </span>
                <span style={{ padding: '4px 12px', borderRadius: '20px', background: 'rgba(255, 255, 255, 0.05)', color: 'var(--text-secondary)', fontSize: '0.8rem', fontWeight: '600' }}>
                  Premium Plan
                </span>
              </div>
            </div>
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '24px' }}>
            <Card title="Personal Information">
              <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
                <Input 
                  label="Full Name" 
                  value={profile.name} 
                  icon={User}
                  onChange={(e) => setProfile({ ...profile, name: e.target.value })}
                />
                <Input 
                  label="Email Address" 
                  value={profile.email} 
                  icon={Mail}
                  readOnly
                  style={{ opacity: 0.7 }}
                />
              </div>
            </Card>

            <Card title="Security & Subscription">
              <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '16px', borderRadius: '12px', background: 'rgba(255, 255, 255, 0.02)', border: '1px solid var(--border-color)' }}>
                  <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
                    <div style={{ padding: '8px', borderRadius: '8px', background: 'rgba(99, 102, 241, 0.1)', color: 'var(--accent-color)' }}>
                      <Shield size={18} />
                    </div>
                    <div>
                      <p style={{ fontSize: '0.9rem', fontWeight: '600' }}>Two-Factor Auth</p>
                      <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>Status: Active</p>
                    </div>
                  </div>
                  <Button variant="secondary" style={{ padding: '6px 12px', fontSize: '0.8rem' }}>Manage</Button>
                </div>

                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '16px', borderRadius: '12px', background: 'rgba(255, 255, 255, 0.02)', border: '1px solid var(--border-color)' }}>
                  <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
                    <div style={{ padding: '8px', borderRadius: '8px', background: 'rgba(16, 185, 129, 0.1)', color: 'var(--success)' }}>
                      <CreditCard size={18} />
                    </div>
                    <div>
                      <p style={{ fontSize: '0.9rem', fontWeight: '600' }}>Subscription</p>
                      <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>Renews May 12, 2026</p>
                    </div>
                  </div>
                  <Button variant="secondary" style={{ padding: '6px 12px', fontSize: '0.8rem' }}>Billing</Button>
                </div>
              </div>
            </Card>
          </div>
          
          <div style={{ marginTop: '32px', display: 'flex', justifyContent: 'flex-end', gap: '12px' }}>
             <Button 
               style={{ padding: '12px 32px' }} 
               onClick={handleUpdateProfile}
               loading={loading}
             >
               Save Changes
             </Button>
          </div>
        </motion.div>
      </div>
    </Layout>
  );
};

export default Profile;
