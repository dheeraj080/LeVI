import React from 'react';
import { format, startOfMonth, endOfMonth, subMonths, addMonths } from 'date-fns';
import { ChevronLeft, ChevronRight, Calendar } from 'lucide-react';
import { Button } from './ui';

const PeriodSelector = ({ currentPeriod, onChange }) => {
  const handlePrev = () => {
    onChange(subMonths(currentPeriod, 1));
  };

  const handleNext = () => {
    onChange(addMonths(currentPeriod, 1));
  };

  return (
    <div style={{ 
      display: 'flex', 
      alignItems: 'center', 
      gap: '12px', 
      background: 'var(--panel-color)', 
      padding: '6px', 
      borderRadius: '12px',
      border: '1px solid var(--border-color)'
    }}>
      <button 
        onClick={handlePrev}
        style={{ background: 'transparent', border: 'none', color: 'white', cursor: 'pointer', display: 'flex', alignItems: 'center', padding: '4px' }}
      >
        <ChevronLeft size={20} />
      </button>
      
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', padding: '0 8px', minWidth: '140px', justifyContent: 'center' }}>
        <Calendar size={16} style={{ color: 'var(--accent-color)' }} />
        <span style={{ fontWeight: '600', fontSize: '0.9rem' }}>
          {format(currentPeriod, 'MMMM yyyy')}
        </span>
      </div>

      <button 
        onClick={handleNext}
        style={{ background: 'transparent', border: 'none', color: 'white', cursor: 'pointer', display: 'flex', alignItems: 'center', padding: '4px' }}
      >
        <ChevronRight size={20} />
      </button>
    </div>
  );
};

export default PeriodSelector;
