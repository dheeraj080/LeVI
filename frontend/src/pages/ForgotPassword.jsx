import React, { useState } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { useAuth } from '../context/AuthContext';
import { Button, Input, Card } from '../components/ui';
import { Mail, Key, ShieldCheck, ArrowLeft, CheckCircle } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';

const ForgotPassword = () => {
  const [step, setStep] = useState(1); // 1: Email, 2: Code/NewPass, 3: Success
  const [email, setEmail] = useState('');
  const [code, setCode] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  
  const { forgotPassword, resetPassword } = useAuth();
  const navigate = useNavigate();

  const handleRequestCode = async (e) => {
    e.preventDefault();
    setLoading(true);
    setError('');
    try {
      await forgotPassword(email);
      setStep(2);
    } catch (err) {
      setError('Could not process request. Please check your email and try again.');
    } finally {
      setLoading(false);
    }
  };

  const handleReset = async (e) => {
    e.preventDefault();
    setLoading(true);
    setError('');
    try {
      await resetPassword(email, code, newPassword);
      setStep(3);
      setTimeout(() => navigate('/login'), 3000);
    } catch (err) {
      setError('Invalid code or session. Please try again.');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ minHeight: '100vh', display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'var(--bg-color)' }}>
      <motion.div 
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        style={{ width: '100%', maxWidth: '400px', padding: '20px' }}
      >
        <div style={{ textAlign: 'center', marginBottom: '32px' }}>
          <h1 style={{ fontSize: '1.75rem', fontWeight: '800', letterSpacing: '-0.03em' }}>
            {step === 1 ? 'Reset Password' : step === 2 ? 'Enter Code' : 'Password Reset!'}
          </h1>
          <p style={{ color: 'var(--text-secondary)', marginTop: '8px' }}>
            {step === 1 ? 'Enter your email to receive a reset code' : step === 2 ? `Sent to ${email}` : 'Redirecting to login...'}
          </p>
        </div>

        <Card>
          <AnimatePresence mode="wait">
            {step === 1 && (
              <motion.form 
                key="step1"
                initial={{ opacity: 0, x: -10 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 10 }}
                onSubmit={handleRequestCode} 
                style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}
              >
                <Input 
                  label="Email Address"
                  type="email"
                  icon={Mail}
                  placeholder="name@example.com"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  required
                />
                {error && <p style={{ color: 'var(--danger)', fontSize: '0.85rem' }}>{error}</p>}
                <Button type="submit" disabled={loading} style={{ width: '100%' }}>
                  {loading ? 'Sending...' : 'Send Reset Code'}
                </Button>
              </motion.form>
            )}

            {step === 2 && (
              <motion.form 
                key="step2"
                initial={{ opacity: 0, x: -10 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 10 }}
                onSubmit={handleReset} 
                style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}
              >
                <Input 
                  label="6-Digit Code"
                  type="text"
                  icon={ShieldCheck}
                  placeholder="123456"
                  maxLength={6}
                  value={code}
                  onChange={(e) => setCode(e.target.value)}
                  required
                />
                <Input 
                  label="New Password"
                  type="password"
                  icon={Key}
                  placeholder="••••••••"
                  value={newPassword}
                  onChange={(e) => setNewPassword(e.target.value)}
                  required
                />
                {error && <p style={{ color: 'var(--danger)', fontSize: '0.85rem' }}>{error}</p>}
                <Button type="submit" disabled={loading} style={{ width: '100%' }}>
                  {loading ? 'Updating...' : 'Set New Password'}
                </Button>
                <button 
                   type="button" 
                   onClick={() => setStep(1)}
                   style={{ background: 'none', border: 'none', color: 'var(--text-secondary)', cursor: 'pointer', fontSize: '0.85rem' }}
                >
                   Try a different email
                </button>
              </motion.form>
            )}

            {step === 3 && (
              <motion.div 
                key="step3"
                initial={{ opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                style={{ textAlign: 'center', padding: '20px 0' }}
              >
                <CheckCircle size={48} color="var(--success)" style={{ marginBottom: '16px' }} />
                <p style={{ fontWeight: '500' }}>Your password has been updated.</p>
              </motion.div>
            )}
          </AnimatePresence>
        </Card>

        <div style={{ textAlign: 'center', marginTop: '24px' }}>
          <Link to="/login" style={{ display: 'inline-flex', alignItems: 'center', gap: '8px', color: 'var(--text-secondary)', textDecoration: 'none', fontSize: '0.9rem' }}>
            <ArrowLeft size={16} /> Back to Login
          </Link>
        </div>
      </motion.div>
    </div>
  );
};

export default ForgotPassword;
