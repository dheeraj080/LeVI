import React, { useState } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { useAuth } from '../context/AuthContext';
import { Button, Input, Card } from '../components/ui';
import { Mail, Lock, LogIn, Globe } from 'lucide-react';
import { motion } from 'framer-motion';

import { useSearchParams } from 'react-router-dom';

const Login = () => {
  const [searchParams] = useSearchParams();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [success, setSuccess] = useState(searchParams.get('activated') ? 'Account activated! Please log in.' : '');
  const [loading, setLoading] = useState(false);
  const { login } = useAuth();
  const navigate = useNavigate();

  const handleSubmit = async (e) => {
    e.preventDefault();
    setLoading(true);
    setError('');
    setSuccess('');
    try {
      await login(email, password);
      navigate('/');
    } catch (err) {
      console.error(err);
      setError(err.response?.data?.message || 'Invalid email or password');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ minHeight: '100vh', display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'radial-gradient(circle at top right, #1e1b4b, #050505)' }}>
      <motion.div 
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        style={{ width: '100%', maxWidth: '400px', padding: '20px' }}
      >
        <div style={{ textAlign: 'center', marginBottom: '32px' }}>
          <div style={{ 
            width: '48px', 
            height: '48px', 
            background: 'var(--accent-color)', 
            borderRadius: '12px',
            display: 'inline-flex',
            alignItems: 'center',
            justifyContent: 'center',
            marginBottom: '16px',
            boxShadow: '0 0 20px var(--accent-glow)'
          }}>
            <LogIn color="white" size={24} />
          </div>
          <h1 style={{ fontSize: '1.75rem', fontWeight: '700' }}>Welcome Back</h1>
          <p style={{ color: 'var(--text-secondary)', marginTop: '8px' }}>Log in to your LeVI account</p>
        </div>

        <Card>
          <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
            <Input 
              label="Email Address"
              type="email"
              icon={Mail}
              placeholder="name@example.com"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              required
            />
            <Input 
              label="Password"
              type="password"
              icon={Lock}
              placeholder="••••••••"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
            
            {success && <p style={{ color: 'var(--success)', fontSize: '0.85rem' }}>{success}</p>}
            {error && <p style={{ color: 'var(--danger)', fontSize: '0.85rem' }}>{error}</p>}

            <Button type="submit" disabled={loading} style={{ width: '100%', marginTop: '8px' }}>
              {loading ? 'Logging in...' : 'Sign In'}
            </Button>

            <div style={{ display: 'flex', alignItems: 'center', gap: '16px', margin: '8px 0' }}>
              <div style={{ flex: 1, height: '1px', background: 'var(--border-color)' }} />
              <span style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>OR</span>
              <div style={{ flex: 1, height: '1px', background: 'var(--border-color)' }} />
            </div>

            <Button variant="secondary" type="button" style={{ width: '100%' }}>
              <Globe size={18} /> Continue with GitHub
            </Button>
          </form>
        </Card>

        <p style={{ textAlign: 'center', marginTop: '24px', color: 'var(--text-secondary)', fontSize: '0.9rem' }}>
          Don't have an account? <Link to="/register" style={{ color: 'var(--accent-color)', textDecoration: 'none', fontWeight: '600' }}>Sign Up</Link>
        </p>
      </motion.div>
    </div>
  );
};

export default Login;
