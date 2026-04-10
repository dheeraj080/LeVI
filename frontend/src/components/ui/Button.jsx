import React from 'react';

export const Button = ({ children, variant = 'primary', style, ...props }) => {
  const styles = {
    primary: {
      background: 'var(--accent-color)',
      color: 'white',
      boxShadow: '0 4px 14px 0 var(--accent-glow)',
    },
    secondary: {
      background: 'rgba(255, 255, 255, 0.05)',
      color: 'white',
      border: '1px solid var(--border-color)',
    },
    ghost: {
      background: 'transparent',
      color: 'var(--text-secondary)',
    },
    danger: {
      background: 'rgba(239, 68, 68, 0.1)',
      color: 'var(--danger)',
      border: '1px solid rgba(239, 68, 68, 0.2)',
    }
  };

  const currentStyle = styles[variant];

  return (
    <button
      style={{
        padding: '10px 20px',
        fontWeight: '500',
        fontSize: '0.9rem',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: '8px',
        ...currentStyle,
        ...style
      }}
      {...props}
    >
      {children}
    </button>
  );
};
