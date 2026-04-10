import React from 'react';

export const Input = ({ label, icon: Icon, error, ...props }) => (
  <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', width: '100%' }}>
    {label && (
      <label style={{ fontSize: '0.85rem', color: 'var(--text-secondary)', fontWeight: '500' }}>
        {label}
      </label>
    )}
    <div style={{ position: 'relative', width: '100%' }}>
      {Icon && (
        <Icon 
          size={18} 
          style={{ 
            position: 'absolute', 
            left: '12px', 
            top: '50%', 
            transform: 'translateY(-50%)', 
            color: 'var(--text-secondary)',
            pointerEvents: 'none'
          }} 
        />
      )}
      <input 
        style={{ 
          paddingLeft: Icon ? '40px' : '16px',
          borderColor: error ? 'var(--danger)' : 'var(--border-color)'
        }}
        {...props} 
      />
    </div>
    {error && <p style={{ color: 'var(--danger)', fontSize: '0.75rem', marginTop: '4px' }}>{error}</p>}
  </div>
);
