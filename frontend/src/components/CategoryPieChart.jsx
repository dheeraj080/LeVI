import React from 'react';
import { motion } from 'framer-motion';

const CategoryPieChart = ({ data = [] }) => {
  // If no data, show message
  if (!data || data.length === 0) {
    return (
      <div style={{ height: '200px', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-secondary)', fontSize: '0.9rem' }}>
        No data available
      </div>
    );
  }

  // Calculate total for percentages
  const total = data.reduce((acc, item) => acc + Math.abs(item.amount), 0);
  
  // Calculate segments
  let cumulativePercent = 0;

  const getCoordinatesForPercent = (percent) => {
    const x = Math.cos(2 * Math.PI * percent);
    const y = Math.sin(2 * Math.PI * percent);
    return [x, y];
  };

  const segments = data.map((item) => {
    const percent = Math.abs(item.amount) / total;
    const [startX, startY] = getCoordinatesForPercent(cumulativePercent);
    cumulativePercent += percent;
    const [endX, endY] = getCoordinatesForPercent(cumulativePercent);

    const largeArcFlag = percent > 0.5 ? 1 : 0;

    const pathData = [
      `M ${startX} ${startY}`,
      `A 1 1 0 ${largeArcFlag} 1 ${endX} ${endY}`,
      `L 0 0`,
    ].join(' ');

    return {
      ...item,
      pathData,
      percent: (percent * 100).toFixed(1)
    };
  });

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: '40px', padding: '20px 0' }}>
      <div style={{ position: 'relative', width: '200px', height: '200px' }}>
        <svg viewBox="-1 -1 2 2" style={{ transform: 'rotate(-90deg)', overflow: 'visible' }}>
          {segments.map((segment, i) => (
            <motion.path
              key={i}
              d={segment.pathData}
              fill={segment.categoryIcon || '#6366f1'}
              initial={{ opacity: 0, scale: 0.8 }}
              animate={{ opacity: 1, scale: 1 }}
              transition={{ delay: i * 0.1, duration: 0.5 }}
              whileHover={{ scale: 1.05, filter: 'brightness(1.2)' }}
              style={{ cursor: 'pointer' }}
            >
              <title>{`${segment.categoryName}: ${segment.percent}%`}</title>
            </motion.path>
          ))}
          {/* Inner circle for donut hole */}
          <circle cx="0" cy="0" r="0.65" fill="var(--panel-color)" />
          
          <text 
            x="0" 
            y="0" 
            fill="white" 
            textAnchor="middle" 
            dominantBaseline="middle" 
            style={{ fontSize: '0.2px', fontWeight: '700', transform: 'rotate(90deg)' }}
          >
            Expenses
          </text>
        </svg>
      </div>

      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: '12px' }}>
        {segments.map((segment, i) => (
          <div key={i} style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <div style={{ width: '10px', height: '10px', borderRadius: '50%', background: segment.categoryIcon || '#6366f1' }} />
              <span style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>{segment.categoryName}</span>
            </div>
            <span style={{ fontSize: '0.85rem', fontWeight: '600' }}>{segment.percent}%</span>
          </div>
        ))}
      </div>
    </div>
  );
};

export default CategoryPieChart;
