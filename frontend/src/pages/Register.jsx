import React, { useState } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { useAuth } from '../context/AuthContext';
import { Button, Input, Card } from '../components/ui';
import { Mail, Lock, User, UserPlus, ArrowLeft, CheckCircle, ArrowRight } from 'lucide-react';
import { motion } from 'framer-motion';

const Register = () => {
  const [formData, setFormData] = useState({
    firstName: '',
    lastName: '',
    email: '',
    password: '',
    confirmPassword: ''
  });
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [success, setSuccess] = useState(false);
  
  const { register } = useAuth();
  const navigate = useNavigate();

  const handleChange = (e) => {
    setFormData({ ...formData, [e.target.name]: e.target.value });
  };

  const handleSubmit = async (e) => {
    e.preventDefault();
    if (formData.password !== formData.confirmPassword) {
      return setError('Passwords do not match');
    }

    setLoading(true);
    setError('');
    
    try {
      const userPayload = {
        firstName: formData.firstName,
        lastName: formData.lastName,
        email: formData.email,
        password: formData.password
      };
      
      await register(userPayload);
      setSuccess(true);
    } catch (err) {
      console.error(err);
      setError(err.response?.data?.message || 'Registration failed. Please try again.');
    } finally {
      setLoading(false);
    }
  };

  if (success) {
    return (
      <div style={{ minHeight: '100vh', display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'var(--bg-color)' }}>
        <motion.div 
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          style={{ maxWidth: '480px', width: '100%', padding: '20px' }}
        >
          <div style={{ textAlign: 'center', padding: '40px' }}>
            <div style={{ 
              width: '80px', height: '80px', background: 'rgba(48, 209, 88, 0.1)', 
              borderRadius: '24px', display: 'inline-flex', alignItems: 'center', 
              justifyContent: 'center', marginBottom: '32px', color: 'var(--success)'
            }}>
              <CheckCircle size={40} />
            </div>
            <h2 style={{ fontSize: '2.5rem', fontWeight: '800', letterSpacing: '-0.04em', marginBottom: '16px' }}>Verify yours.</h2>
            <p style={{ color: 'var(--text-secondary)', lineHeight: '1.6', fontSize: '1.1rem', marginBottom: '40px' }}>
              We've sent an activation link to <strong>{formData.email}</strong>.<br />Secure your account to begin.
            </p>
            <Link to="/login" style={{ textDecoration: 'none' }}>
              <Button style={{ width: '100%', padding: '16px' }}>Return to Login <ArrowRight size={18} /></Button>
            </Link>
          </div>
        </motion.div>
      </div>
    );
  }

  return (
    <div style={{ 
      minHeight: '100vh', 
      display: 'grid', 
      gridTemplateColumns: '480px 1fr',
      background: 'var(--bg-color)',
      color: 'var(--text-primary)'
    }}>
      {/* Form Side (LHS) */}
      <div style={{ 
        background: 'var(--bg-color)', 
        display: 'flex', 
        flexDirection: 'column', 
        justifyContent: 'center', 
        padding: '0 60px',
        borderRight: '1px solid var(--border-color)',
        position: 'relative'
      }}>
        <div style={{ position: 'absolute', top: '40px', left: '40px' }}>
           <Link to="/" style={{ display: 'flex', alignItems: 'center', gap: '8px', textDecoration: 'none', color: 'var(--text-secondary)', fontSize: '0.9rem' }}>
              <ArrowLeft size={16} /> Back
           </Link>
        </div>

        <motion.div 
          initial={{ opacity: 0, x: -20 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.5 }}
          style={{ maxWidth: '360px', width: '100%' }}
        >
          <div style={{ marginBottom: '40px' }}>
            <h1 style={{ fontSize: '2rem', fontWeight: '800', letterSpacing: '-0.03em', marginBottom: '12px' }}>Get started</h1>
            <p style={{ color: 'var(--text-secondary)', fontSize: '0.95rem' }}>Start your 14-day free premium trial</p>
          </div>

          <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '16px' }}>
              <Input 
                label="First Name"
                name="firstName"
                placeholder="John"
                value={formData.firstName}
                onChange={handleChange}
                required
              />
              <Input 
                label="Last Name"
                name="lastName"
                placeholder="Doe"
                value={formData.lastName}
                onChange={handleChange}
                required
              />
            </div>
            <Input 
              label="Email Address"
              type="email"
              name="email"
              placeholder="you@example.com"
              value={formData.email}
              onChange={handleChange}
              required
            />
            <Input 
              label="Password"
              type="password"
              name="password"
              placeholder="Min. 8 characters"
              value={formData.password}
              onChange={handleChange}
              required
            />
            <Input 
              label="Confirm Password"
              type="password"
              name="confirmPassword"
              placeholder="Confirm password"
              value={formData.confirmPassword}
              onChange={handleChange}
              required
            />
            
            {error && <p style={{ color: 'var(--danger)', fontSize: '0.85rem', textAlign: 'center' }}>{error}</p>}

            <Button type="submit" disabled={loading} style={{ width: '100%', marginTop: '12px', padding: '14px' }}>
              {loading ? 'Creating account...' : 'Create account'}
            </Button>
          </form>

          <p style={{ textAlign: 'center', marginTop: '40px', color: 'var(--text-secondary)', fontSize: '0.9rem' }}>
            Already have an account? <Link to="/login" style={{ color: 'var(--text-primary)', textDecoration: 'none', fontWeight: '600' }}>Login <ArrowRight size={14} style={{ display: 'inline', verticalAlign: 'middle' }} /></Link>
          </p>
        </motion.div>
      </div>

       {/* Visual Side (RHS) */}
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
          position: 'absolute', bottom: '-10%', left: '-10%', 
          width: '600px', height: '600px', 
          background: 'radial-gradient(circle, rgba(255,255,255,0.03) 0%, transparent 70%)',
          zIndex: 0
        }} />
        
        <motion.div
           initial={{ opacity: 0, x: 20 }}
           animate={{ opacity: 1, x: 0 }}
           transition={{ duration: 0.8 }}
           style={{ zIndex: 1, textAlign: 'right', display: 'flex', flexDirection: 'column', alignItems: 'flex-end' }}
        >
          <div style={{ 
            width: '40px', height: '40px', background: 'white', 
            color: 'black', borderRadius: '10px', display: 'flex', 
            alignItems: 'center', justifyContent: 'center', fontWeight: 'bold',
            fontSize: '1.2rem', marginBottom: '40px'
          }}>L</div>
          
          <h2 style={{ fontSize: '3.5rem', fontWeight: '800', letterSpacing: '-0.04em', lineHeight: '1.1', marginBottom: '24px' }}>
            Control your<br /><span style={{ opacity: 0.4 }}>financial destiny.</span>
          </h2>
          <p style={{ fontSize: '1.1rem', color: 'var(--text-secondary)', maxWidth: '400px', lineHeight: '1.6' }}>
            Join a community of disciplined wealth builders. No bloat, just the data you need to grow.
          </p>
        </motion.div>

        {/* Minimalist List Card */}
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
            width: '320px',
            alignSelf: 'flex-end',
            zIndex: 1
          }}
        >
           <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--text-secondary)', fontSize: '0.85rem' }}>Growth</span>
                <span style={{ color: 'var(--success)', fontSize: '0.85rem' }}>Available</span>
              </div>
              <div style={{ height: '8px', width: '100%', background: 'rgba(255,255,255,0.05)', borderRadius: '4px', overflow: 'hidden' }}>
                <motion.div 
                  initial={{ width: 0 }}
                  animate={{ width: '70%' }}
                  transition={{ delay: 1, duration: 1.5 }}
                  style={{ height: '100%', background: 'white' }} 
                />
              </div>
              <p style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>You've reached 70% of your savings goal this month.</p>
           </div>
        </motion.div>
      </div>
    </div>
  );
};

export default Register;
