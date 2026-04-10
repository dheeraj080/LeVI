import React from 'react';

export const Card = ({ children, title, action, style, className = "" }) => (
  <div className={`glass ${className}`} style={{ padding: '24px', ...style }}>
    {(title || action) && (
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
        {title && <h3 style={{ fontSize: '1.1rem', fontWeight: '600', letterSpacing: '-0.2px' }}>{title}</h3>}
        {action}
      </div>
    )}
    {children}
  </div>
);
