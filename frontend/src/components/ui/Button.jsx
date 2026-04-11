import React from 'react';

export const Button = ({ children, variant = 'primary', style, ...props }) => {
  const styles = {
    primary: {
      background: 'var(--text-primary)',
      color: 'var(--sidebar-bg)',
      boxShadow: 'var(--shadow-sm)',
      border: '1px solid var(--text-primary)',
    },
    secondary: {
      background: 'var(--glass-bg)',
      color: 'var(--text-primary)',
      border: '1px solid var(--border-color)',
    },
    ghost: {
      background: 'transparent',
      color: 'var(--text-secondary)',
    },
    danger: {
      background: 'rgba(255, 69, 58, 0.1)',
      color: 'var(--accent-pink)',
      border: '1px solid rgba(255, 69, 58, 0.2)',
    }
  };

  const currentStyle = styles[variant];

  return (
    <button
      style={{
        padding: '8px 16px',
        fontWeight: '500',
        fontSize: '0.85rem',
        letterSpacing: '-0.01em',
        borderRadius: '6px',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: '8px',
        transition: 'var(--transition-smooth)',
        ...currentStyle,
        ...style
      }}
      {...props}
    >
      {children}
    </button>
  );
};
