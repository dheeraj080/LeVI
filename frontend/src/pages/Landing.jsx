import React, { useEffect } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { 
  ArrowRight, 
  Shield, 
  Zap, 
  BarChart3, 
  Smartphone, 
  Globe, 
  Lock,
  Plus,
  Star,
  Check
} from 'lucide-react';
import { Button, Card } from '../components/ui';
import { useAuth } from '../context/AuthContext';
import { useTheme } from '../context/ThemeContext';

const Nav = () => {
  const { user } = useAuth();
  
  return (
    <nav style={{
      position: 'fixed',
      top: 0,
      left: 0,
      right: 0,
      height: '72px',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      padding: '0 80px',
      background: 'rgba(var(--bg-color-rgb), 0.7)',
      backdropFilter: 'blur(24px)',
      zIndex: 1000,
      borderBottom: '1px solid var(--border-color)'
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
        <div style={{ 
          width: '32px', height: '32px', background: 'var(--text-primary)', 
          color: 'var(--bg-color)', borderRadius: '8px', display: 'flex', 
          alignItems: 'center', justifyContent: 'center', fontWeight: 'bold' 
        }}>L</div>
        <span style={{ fontSize: '1.2rem', fontWeight: '700', letterSpacing: '-0.5px' }}>LeVI</span>
      </div>
      
      <div style={{ display: 'flex', alignItems: 'center', gap: '40px' }}>
        <a href="#features" style={{ color: 'var(--text-secondary)', textDecoration: 'none', fontSize: '0.85rem', fontWeight: '500', letterSpacing: '0.02em' }}>PRODUCT</a>
        <a href="#security" style={{ color: 'var(--text-secondary)', textDecoration: 'none', fontSize: '0.85rem', fontWeight: '500', letterSpacing: '0.02em' }}>SECURITY</a>
        <a href="#pricing" style={{ color: 'var(--text-secondary)', textDecoration: 'none', fontSize: '0.85rem', fontWeight: '500', letterSpacing: '0.02em' }}>PRICING</a>
        <div style={{ height: '16px', width: '1px', background: 'var(--border-color)' }} />
        {user ? (
          <Link to="/dashboard">
            <Button style={{ padding: '8px 20px', fontSize: '0.85rem' }}>Dashboard <ArrowRight size={14} /></Button>
          </Link>
        ) : (
          <div style={{ display: 'flex', gap: '12px' }}>
            <Link to="/login">
              <Button variant="ghost" style={{ fontSize: '0.85rem' }}>Login</Button>
            </Link>
            <Link to="/register">
              <Button style={{ padding: '8px 24px', fontSize: '0.85rem' }}>Get Started</Button>
            </Link>
          </div>
        )}
      </div>
    </nav>
  );
};

const FeatureCard = ({ icon: Icon, title, description, color }) => (
  <Card style={{ padding: '40px', background: 'var(--glass-bg)', border: '1px solid var(--border-color)' }}>
    <div style={{ 
      width: '48px', height: '48px', borderRadius: '12px', 
      background: `color-mix(in srgb, ${color} 10%, transparent)`, 
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      marginBottom: '32px', color: color,
      border: `1px solid color-mix(in srgb, ${color} 20%, transparent)`
    }}>
      <Icon size={20} />
    </div>
    <h3 style={{ fontSize: '1.2rem', fontWeight: '600', marginBottom: '16px', letterSpacing: '-0.02em' }}>{title}</h3>
    <p style={{ color: 'var(--text-secondary)', lineHeight: '1.7', fontSize: '0.9rem' }}>{description}</p>
  </Card>
);

const Landing = () => {
  const { user } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    // We'll keep the redirect but allow viewing if preferred
    // For now, let's keep it for a clean flow
    if (user) navigate('/dashboard');
  }, [user, navigate]);

  return (
    <div style={{ background: 'var(--bg-color)', color: 'var(--text-primary)', minHeight: '100vh', scrollBehavior: 'smooth' }}>
      <Nav />
      
      {/* Hero Section - The Apple/Linear approach */}
      <section style={{ 
        padding: '220px 80px 120px', 
        display: 'flex', 
        flexDirection: 'column', 
        alignItems: 'center', 
        textAlign: 'center',
        position: 'relative',
        overflow: 'hidden'
      }}>
        {/* Cinematic Backdrop */}
        <div style={{ 
          position: 'absolute', top: '0', left: '0', right: '0', height: '100%',
          background: 'radial-gradient(circle at 50% 30%, rgba(255,255,255,0.03) 0%, transparent 60%)',
          zIndex: 0, pointerEvents: 'none'
        }} />
        
        <motion.div
           initial={{ opacity: 0, y: 40 }}
           animate={{ opacity: 1, y: 0 }}
           transition={{ duration: 1, ease: [0.16, 1, 0.3, 1] }} // Custom ease-out
           style={{ zIndex: 1, maxWidth: '1000px' }}
        >
          <motion.div 
            initial={{ opacity: 0, scale: 0.9 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ delay: 0.2 }}
            style={{ 
              padding: '6px 14px', borderRadius: '100px', background: 'var(--panel-color)', 
              border: '1px solid var(--border-color)', fontSize: '0.75rem', fontWeight: '600',
              color: 'var(--text-secondary)', marginBottom: '40px', display: 'inline-flex',
              alignItems: 'center', gap: '10px', boxShadow: '0 4px 12px rgba(0,0,0,0.2)'
            }}
          >
            <span style={{ color: 'var(--accent-purple)' }}><Star size={12} fill="var(--accent-purple)" /></span>
            Voted #1 Premium Tracker of 2026
          </motion.div>

          <h1 style={{ 
            fontSize: 'clamp(3.5rem, 9vw, 6.5rem)', fontWeight: '800', lineHeight: '0.95', 
            letterSpacing: '-0.05em', marginBottom: '40px', maxWidth: '900px',
            color: 'var(--text-primary)'
          }}>
            Invest in your<br /><span style={{ color: 'var(--accent-color)', opacity: 0.6 }}>Financial Clarity.</span>
          </h1>
          
          <p style={{ 
            fontSize: '1.35rem', color: 'var(--text-secondary)', maxWidth: '640px', 
            margin: '0 auto 56px', lineHeight: '1.6', fontWeight: '400'
          }}>
            LeVI is the definitive interface for modern wealth management. Designed for those who demand precision and elegance.
          </p>

          <div style={{ display: 'flex', gap: '16px', justifyContent: 'center' }}>
            <Link to="/register">
              <Button style={{ padding: '16px 48px', fontSize: '1rem', borderRadius: '100px' }}>Start managing</Button>
            </Link>
            <Button variant="secondary" style={{ padding: '16px 48px', fontSize: '1rem', borderRadius: '100px', background: 'transparent' }}>View enterprise</Button>
          </div>
        </motion.div>

        {/* The "Floating Glass" Mockup */}
        <motion.div
          initial={{ opacity: 0, y: 120 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.5, duration: 1.2, ease: [0.16, 1, 0.3, 1] }}
          style={{ 
            marginTop: '120px', width: '100%', maxWidth: '1400px', 
            borderRadius: '32px', border: '1px solid var(--border-color)',
            background: 'linear-gradient(180deg, rgba(255,255,255,0.03) 0%, rgba(255,255,255,0.01) 100%)',
            padding: '12px',
            boxShadow: '0 60px 120px -20px rgba(0,0,0,0.6)',
            position: 'relative'
          }}
        >
          <div style={{ borderRadius: '20px', overflow: 'hidden', border: '1px solid var(--border-color)' }}>
            <img 
              src="https://images.unsplash.com/photo-1620712943543-bcc4628c9757?auto=format&fit=crop&q=80&w=2000" 
              alt="Elite App Dashboard" 
              style={{ width: '100%', height: 'auto', display: 'block', filter: 'brightness(0.8) contrast(1.1)' }}
            />
          </div>
        </motion.div>
      </section>

      {/* Trusted By / Logos Section - Ultra Subtle */}
      <section style={{ padding: '60px 80px', textAlign: 'center', opacity: 0.4 }}>
        <p style={{ fontSize: '0.75rem', letterSpacing: '0.2em', fontWeight: '600', marginBottom: '40px' }}>TRUSTED BY GLOBAL VISIONARIES</p>
        <div style={{ display: 'flex', justifyContent: 'center', gap: '80px', filter: 'grayscale(100%) brightness(2)' }}>
           <span style={{ fontSize: '1.5rem', fontWeight: '800', letterSpacing: '-0.05em' }}>FORBES</span>
           <span style={{ fontSize: '1.5rem', fontWeight: '800', letterSpacing: '-0.05em' }}>BLOOMBERG</span>
           <span style={{ fontSize: '1.5rem', fontWeight: '800', letterSpacing: '-0.05em' }}>VERGE</span>
           <span style={{ fontSize: '1.5rem', fontWeight: '800', letterSpacing: '-0.05em' }}>REUTERS</span>
        </div>
      </section>

      {/* Value Proposition Grid - Bento Inspired */}
      <section id="features" style={{ padding: '160px 80px', maxWidth: '1600px', margin: '0 auto' }}>
        <div style={{ marginBottom: '100px' }}>
          <h2 style={{ fontSize: '3.5rem', fontWeight: '800', letterSpacing: '-0.04em', marginBottom: '24px' }}>Engineered for excellence.</h2>
          <p style={{ color: 'var(--text-secondary)', fontSize: '1.2rem', maxWidth: '600px' }}>We stripped away the noise to leave you with pure, actionable financial intelligence.</p>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '24px' }}>
          <FeatureCard 
            icon={Zap} 
            title="Real-time Execution" 
            description="Witness your sub-second transaction processing powered by our high-performance Java backend engine."
            color="var(--accent-orange)"
          />
          <FeatureCard 
            icon={BarChart3} 
            title="Data Sculpting" 
            description="Our charts aren't just displays—they are masterpieces of your financial health, rendered in pure SVG."
            color="var(--accent-purple)"
          />
          <FeatureCard 
            icon={Smartphone} 
            title="Ubiquitous Edge" 
            description="Designed for the iPhone, perfected for the Desktop. Your wealth is always one glance away."
            color="var(--accent-pink)"
          />
          <div style={{ gridColumn: 'span 2' }}>
            <Card style={{ padding: '80px', display: 'flex', gap: '60px', alignItems: 'center', background: 'var(--panel-color)' }}>
               <div style={{ flex: 1 }}>
                  <h3 style={{ fontSize: '2rem', fontWeight: '700', marginBottom: '20px', color: 'var(--text-primary)' }}>Privacy is the ultimate luxury.</h3>
                  <p style={{ color: 'var(--text-secondary)', lineHeight: '1.7' }}>
                    LeVI is built on a private-first architecture. We don't sell your data, we don't track your location. Your financial business stays your financial business.
                  </p>
                  <div style={{ marginTop: '40px', display: 'flex', flexDirection: 'column', gap: '12px' }}>
                     {[ 'Zero-Knowledge Encryption', 'Independently Audited', 'Multi-Factor Verification' ].map(text => (
                       <div key={text} style={{ display: 'flex', alignItems: 'center', gap: '12px', fontSize: '0.9rem', color: 'var(--text-secondary)' }}>
                          <Check size={16} color="var(--success)" />
                          <span>{text}</span>
                       </div>
                     ))}
                  </div>
               </div>
               <div style={{ flex: 1, display: 'flex', justifyContent: 'center' }}>
                  <Shield size={180} color="var(--success)" opacity={0.1} strokeWidth={1} />
               </div>
            </Card>
          </div>
          <FeatureCard 
            icon={Globe} 
            title="Global Compliance" 
            description="Fully compliant with modern digital security standards across Europe and the Americas."
            color="var(--accent-color)"
          />
        </div>
      </section>

      {/* CTA Section - The "One Last Thing" */}
      <section style={{ padding: '120px 80px', textAlign: 'center' }}>
         <Card style={{ padding: '120px 40px', background: '#050505', border: '1px solid var(--border-color)', position: 'relative', overflow: 'hidden' }}>
            <div style={{ position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)', width: '400px', height: '400px', background: 'radial-gradient(circle, rgba(255,255,255,0.05) 0%, transparent 70%)', zIndex: 0 }} />
            <div style={{ position: 'relative', zIndex: 1 }}>
              <h2 style={{ fontSize: '4rem', fontWeight: '800', letterSpacing: '-0.05em', marginBottom: '24px' }}>Ready for clarity?</h2>
              <p style={{ color: 'var(--text-secondary)', fontSize: '1.2rem', marginBottom: '48px', maxWidth: '500px', margin: '0 auto 48px' }}>Join the elite circle of users refining their financial future today.</p>
              <Link to="/register">
                <Button style={{ padding: '18px 60px', borderRadius: '100px', fontSize: '1.1rem' }}>Create your account</Button>
              </Link>
            </div>
         </Card>
      </section>

      {/* Footer - Precise and Quiet */}
      <footer style={{ 
        padding: '120px 80px 80px', 
        maxWidth: '1600px', 
        margin: '0 auto',
        borderTop: '1px solid var(--border-color)'
      }}>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '80px' }}>
          <div style={{ gridColumn: 'span 2' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '12px', marginBottom: '32px' }}>
              <div style={{ 
                width: '32px', height: '32px', background: 'var(--text-primary)', 
                color: 'var(--bg-color)', borderRadius: '8px', display: 'flex', 
                alignItems: 'center', justifyContent: 'center', fontWeight: 'bold' 
              }}>L</div>
              <span style={{ fontSize: '1.2rem', fontWeight: '700' }}>LeVI</span>
            </div>
            <p style={{ color: 'var(--text-secondary)', maxWidth: '320px', lineHeight: '1.6', fontSize: '0.95rem' }}>
              Redefining the relationship between human design and financial intelligence. Available worldwide.
            </p>
          </div>
          <div>
            <h4 style={{ fontSize: '0.75rem', fontWeight: '700', letterSpacing: '0.1em', color: 'var(--text-secondary)', marginBottom: '32px', textTransform: 'uppercase' }}>Resources</h4>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', fontSize: '0.9rem' }}>
              <span>Manifesto</span>
              <span>Design System</span>
              <span>Technical Support</span>
              <span>Privacy Ethics</span>
            </div>
          </div>
          <div>
            <h4 style={{ fontSize: '0.75rem', fontWeight: '700', letterSpacing: '0.1em', color: 'var(--text-secondary)', marginBottom: '32px', textTransform: 'uppercase' }}>Social</h4>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', fontSize: '0.9rem' }}>
              <span>Twitter / X</span>
              <span>Instagram</span>
              <span>LinkedIn</span>
            </div>
          </div>
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: '120px', color: 'var(--text-secondary)', fontSize: '0.8rem', fontWeight: '500' }}>
            <span>© 2026 LEVI SYSTEMS INC.</span>
            <div style={{ display: 'flex', gap: '40px' }}>
              <span>LEGAL</span>
              <span>PRIVACY</span>
              <span>SECURITY</span>
            </div>
        </div>
      </footer>
    </div>
  );
};

export default Landing;
