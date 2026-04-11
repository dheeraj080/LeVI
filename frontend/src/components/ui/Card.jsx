import React from 'react';

export const Card = ({ children, title, action, style, noPadding = false, className = "" }) => (
  <div 
    className={className} 
    style={{ 
      background: 'var(--panel-color)',
      border: '1px solid var(--border-color)',
      borderRadius: '12px',
      padding: noPadding ? '0' : '24px',
      boxShadow: 'var(--shadow-lg)',
      ...style 
    }}
  >
    {(title || action) && (
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: noPadding ? '0' : '20px', padding: noPadding ? '24px 24px 0 24px' : '0' }}>
        {title && <h3 style={{ fontSize: '1.05rem', fontWeight: '500', letterSpacing: '-0.02em', color: 'var(--text-primary)' }}>{title}</h3>}
        {action}
      </div>
    )}
    {children}
  </div>
);
