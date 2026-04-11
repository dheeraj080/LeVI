import React, { useState } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { useAuth } from '../context/AuthContext';
import { Button, Input, Card } from '../components/ui';
import { Mail, Lock, LogIn, Globe, ArrowRight } from 'lucide-react';
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
      navigate('/dashboard');
    } catch (err) {
      console.error(err);
      setError(err.response?.data?.message || 'Invalid email or password');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ 
      minHeight: '100vh', 
      display: 'grid', 
      gridTemplateColumns: '1fr 480px',
      background: 'var(--bg-color)',
      color: 'var(--text-primary)',
      overflow: 'hidden'
    }}>
      {/* Visual Side (LHS) */}
      <div style={{ 
        position: 'relative', 
        background: '#050505',
        display: 'flex',
        flexDirection: 'column',
        justifyContent: 'center',
        padding: '80px',
        overflow: 'hidden'
      }}>
        {/* Abstract Glow Background */}
        <div style={{ 
          position: 'absolute', top: '-10%', right: '-10%', 
          width: '600px', height: '600px', 
          background: 'radial-gradient(circle, rgba(255,255,255,0.03) 0%, transparent 70%)',
          zIndex: 0
        }} />
        
        <motion.div
           initial={{ opacity: 0, x: -20 }}
           animate={{ opacity: 1, x: 0 }}
           transition={{ duration: 0.8 }}
           style={{ zIndex: 1 }}
        >
          <div style={{ 
            width: '40px', height: '40px', background: 'white', 
            color: 'black', borderRadius: '10px', display: 'flex', 
            alignItems: 'center', justifyContent: 'center', fontWeight: 'bold',
            fontSize: '1.2rem', marginBottom: '40px'
          }}>L</div>
          
          <h2 style={{ fontSize: '3.5rem', fontWeight: '800', letterSpacing: '-0.04em', lineHeight: '1.1', marginBottom: '24px' }}>
            Financial clarity,<br /><span style={{ opacity: 0.4 }}>beautifully designed.</span>
          </h2>
          <p style={{ fontSize: '1.1rem', color: 'var(--text-secondary)', maxWidth: '400px', lineHeight: '1.6' }}>
            Join thousands of modern users who have mastered their wealth with the world's most elegant expense tracker.
          </p>
        </motion.div>

        {/* Floating Stat Preview (Minimalist) */}
        <motion.div 
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.4, duration: 0.8 }}
          style={{ 
            marginTop: '80px', 
            padding: '24px', 
            background: 'rgba(255,255,255,0.02)', 
            border: '1px solid rgba(255,255,255,0.08)',
            borderRadius: '20px',
            width: 'fit-content',
            zIndex: 1
          }}
        >
           <div style={{ display: 'flex', gap: '32px', alignItems: 'center' }}>
              <div>
                <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)', textTransform: 'uppercase', letterSpacing: '0.05em', marginBottom: '4px' }}>Efficiency</p>
                <p style={{ fontSize: '1.5rem', fontWeight: '700' }}>+12.4%</p>
              </div>
              <div style={{ height: '40px', width: '1px', background: 'rgba(255,255,255,0.1)' }} />
              <div>
                <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)', textTransform: 'uppercase', letterSpacing: '0.05em', marginBottom: '4px' }}>Health</p>
                <p style={{ fontSize: '1.5rem', fontWeight: '700', color: 'var(--success)' }}>Good</p>
              </div>
           </div>
        </motion.div>
      </div>

      {/* Form Side (RHS) */}
      <div style={{ 
        background: 'var(--bg-color)', 
        display: 'flex', 
        flexDirection: 'column', 
        justifyContent: 'center', 
        padding: '0 60px',
        borderLeft: '1px solid var(--border-color)',
        position: 'relative'
      }}>
        <div style={{ position: 'absolute', top: '40px', right: '40px' }}>
           <Link to="/register" style={{ textDecoration: 'none' }}>
              <Button variant="ghost" style={{ fontSize: '0.85rem' }}>Create business account</Button>
           </Link>
        </div>

        <motion.div 
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.5 }}
          style={{ maxWidth: '360px', width: '100%' }}
        >
          <div style={{ marginBottom: '40px' }}>
            <h1 style={{ fontSize: '2rem', fontWeight: '800', letterSpacing: '-0.03em', marginBottom: '12px' }}>Welcome back</h1>
            <p style={{ color: 'var(--text-secondary)', fontSize: '0.95rem' }}>Enter your details to access your account</p>
          </div>

          <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
            <Input 
              label="Email Address"
              type="email"
              placeholder="name@company.com"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              required
            />
            <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <label style={{ fontSize: '0.85rem', fontWeight: '500', color: 'var(--text-secondary)' }}>Password</label>
                <Link to="/forgot-password" style={{ fontSize: '0.8rem', color: 'var(--text-secondary)', textDecoration: 'none' }}>
                  Forgot password?
                </Link>
              </div>
              <Input 
                type="password"
                placeholder="••••••••"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
              />
            </div>
            
            {success && <p style={{ color: 'var(--success)', fontSize: '0.85rem' }}>{success}</p>}
            {error && <p style={{ color: 'var(--danger)', fontSize: '0.85rem' }}>{error}</p>}

            <Button type="submit" disabled={loading} style={{ width: '100%', marginTop: '12px', padding: '14px' }}>
              {loading ? 'Signing in...' : 'Sign in to LeVI'}
            </Button>

            <div style={{ display: 'flex', alignItems: 'center', gap: '16px', margin: '8px 0' }}>
              <div style={{ flex: 1, height: '1px', background: 'var(--border-color)' }} />
              <span style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>OR</span>
              <div style={{ flex: 1, height: '1px', background: 'var(--border-color)' }} />
            </div>

            <Button variant="secondary" type="button" style={{ width: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center', gap: '10px' }}>
              <Globe size={18} /> Continue with GitHub
            </Button>
          </form>

          <p style={{ textAlign: 'center', marginTop: '40px', color: 'var(--text-secondary)', fontSize: '0.9rem' }}>
            New to LeVI? <Link to="/register" style={{ color: 'var(--text-primary)', textDecoration: 'none', fontWeight: '600' }}>Create account <ArrowRight size={14} style={{ display: 'inline', verticalAlign: 'middle' }} /></Link>
          </p>
        </motion.div>
      </div>
    </div>
  );
};

export default Login;
