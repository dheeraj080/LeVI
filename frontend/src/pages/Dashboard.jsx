import React, { useEffect, useState } from 'react';
import { Layout } from '../components/Layout';
import { Card, Button } from '../components/ui';
import { 
  Trophy, 
  Wallet, 
  TrendingUp, 
  TrendingDown, 
  ChevronLeft, 
  ChevronRight, 
  PieChart, 
  ArrowUpRight, 
  ArrowDownLeft,
  Mail,
  Lock,
  User,
  ShieldAlert,
  CreditCard,
  Target
} from 'lucide-react';
import api from '../api';
import { motion } from 'framer-motion';

const StatCard = ({ icon: Icon, value, label, color }) => (
  <Card style={{ padding: '24px' }}>
    <div style={{ display: 'flex', alignItems: 'center', gap: '20px' }}>
      <div style={{ 
        width: '48px', 
        height: '48px', 
        borderRadius: '12px', 
        background: `color-mix(in srgb, ${color} 15%, transparent)`, 
        border: `1px solid color-mix(in srgb, ${color} 30%, transparent)`,
        display: 'flex', 
        alignItems: 'center', 
        justifyContent: 'center',
        boxShadow: `0 4px 20px color-mix(in srgb, ${color} 20%, transparent)`
      }}>
        <Icon size={24} color={color} />
      </div>
      <div>
        <h3 style={{ fontSize: '1.75rem', fontWeight: '600', letterSpacing: '-0.02em', lineHeight: 1, color: 'var(--text-primary)' }}>{value}</h3>
        <p style={{ fontSize: '0.85rem', color: 'var(--text-secondary)', marginTop: '6px', fontWeight: '500' }}>{label}</p>
      </div>
    </div>
  </Card>
);

const ActivityItem = ({ icon: Icon, title, time, color }) => (
  <div style={{ display: 'flex', gap: '16px' }}>
    <div style={{ 
      width: '32px', 
      height: '32px', 
      borderRadius: '50%', 
      background: color, 
      display: 'flex', 
      alignItems: 'center', 
      justifyContent: 'center',
      flexShrink: 0
    }}>
      <Icon size={16} color="white" />
    </div>
    <div>
      <p style={{ fontSize: '0.9rem', fontWeight: '500', lineHeight: 1.2 }}>{title}</p>
      <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)', marginTop: '4px' }}>{time}</p>
    </div>
  </div>
);

const Dashboard = () => {
  const [transactions, setTransactions] = useState([]);
  const [summary, setSummary] = useState({ totalBalance: 0, totalIncome: 0, totalExpense: 0 });
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchData();
  }, []);

  const fetchData = async () => {
    try {
      const [txRes, sumRes] = await Promise.all([
        api.get('/transactions'),
        api.get('/transactions/summary')
      ]);
      setTransactions(txRes.data || []);
      setSummary(sumRes.data || { totalBalance: 0, totalIncome: 0, totalExpense: 0 });
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  // Logic to calculate Income vs Expense percentages for the donut
  const incomeVal = summary.totalIncome || 1; // avoid divide by zero
  const expenseVal = summary.totalExpense || 0;
  const expensePercent = Math.min(Math.round((expenseVal / incomeVal) * 100), 100);

  // Logic to build Area Chart path from data
  // We'll group by date and create 5-7 segments
  const generatePath = (type) => {
    if (transactions.length < 2) return type === 'DEBIT' 
      ? "M 0 40 L 0 35 L 20 38 L 40 32 L 60 36 L 80 28 L 100 30 L 100 40 Z" // defaults
      : "M 0 40 L 0 30 L 20 32 L 40 15 L 60 25 L 80 20 L 100 18 L 100 40 Z";

    const filtered = transactions.filter(t => t.type === type).slice(0, 7).reverse();
    if (filtered.length < 2) return type === 'DEBIT' 
      ? "M 0 40 L 0 35 L 20 38 L 40 32 L 60 36 L 80 28 L 100 30 L 100 40 Z"
      : "M 0 40 L 0 30 L 20 32 L 40 15 L 60 25 L 80 20 L 100 18 L 100 40 Z";

    const maxAmt = Math.max(...filtered.map(t => Math.abs(t.amount)), 100);
    const stepX = 100 / (filtered.length - 1);
    
    let path = `M 0 40 L 0 ${40 - (Math.abs(filtered[0].amount) / maxAmt * 30)}`;
    for (let i = 1; i < filtered.length; i++) {
        const y = 40 - (Math.abs(filtered[i].amount) / maxAmt * 30);
        path += ` L ${i * stepX} ${y}`;
    }
    path += ` L 100 40 Z`;
    return path;
  };

  const container = {
    hidden: { opacity: 0 },
    show: { opacity: 1, transition: { staggerChildren: 0.1 } }
  };

  const item = {
    hidden: { opacity: 0, y: 20 },
    show: { opacity: 1, y: 0 }
  };

  return (
    <Layout title="Dashboard">
      <motion.div 
        variants={container}
        initial="hidden"
        animate="show"
        style={{ display: 'flex', flexDirection: 'column', gap: '32px' }}
      >
        {/* Stats Grid */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '24px' }}>
          <motion.div variants={item}><StatCard icon={Wallet} value={`$${(summary.totalBalance || 0).toLocaleString()}`} label="Total Balance" color="var(--accent-color)" /></motion.div>
          <motion.div variants={item}><StatCard icon={TrendingUp} value={`$${(summary.totalIncome || 0).toLocaleString()}`} label="Total Income" color="var(--success)" /></motion.div>
          <motion.div variants={item}><StatCard icon={TrendingDown} value={`$${(summary.totalExpense || 0).toLocaleString()}`} label="Total Expenses" color="var(--danger)" /></motion.div>
          <motion.div variants={item}><StatCard icon={CreditCard} value={`${transactions.length}`} label="Active Records" color="var(--accent-purple)" /></motion.div>
        </div>

        {/* Chart Section */}
        <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr', gap: '24px' }}>
          <motion.div variants={item}>
            <Card title="Income vs Expense Flow">
              <div style={{ height: '300px', width: '100%', position: 'relative', marginTop: '20px' }}>
                <svg viewBox="0 0 100 40" style={{ width: '100%', height: '100%', overflow: 'visible' }}>
                  <defs>
                    <linearGradient id="incGrad" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="var(--accent-color)" stopOpacity="0.4" />
                      <stop offset="100%" stopColor="var(--accent-color)" stopOpacity="0" />
                    </linearGradient>
                    <linearGradient id="expGrad" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="var(--accent-pink)" stopOpacity="0.4" />
                      <stop offset="100%" stopColor="var(--accent-pink)" stopOpacity="0" />
                    </linearGradient>
                  </defs>
                  
                  {/* Real Dynamic Expense Area */}
                  <motion.path 
                    initial={{ opacity: 0, d: "M0,40 L100,40" }}
                    animate={{ opacity: 1, d: generatePath('DEBIT') }}
                    transition={{ duration: 1 }}
                    fill="url(#expGrad)" 
                  />
                  
                  {/* Real Dynamic Income Area */}
                  <motion.path 
                    initial={{ opacity: 0, d: "M0,40 L100,40" }}
                    animate={{ opacity: 1, d: generatePath('CREDIT') }}
                    transition={{ duration: 1.2 }}
                    fill="url(#incGrad)" 
                  />
                  
                  {[0, 25, 50, 75, 100].map(x => (
                    <text key={x} x={x} y="44" fill="var(--text-secondary)" fontSize="2" textAnchor="middle">Period {x}%</text>
                  ))}
                </svg>
              </div>
            </Card>
          </motion.div>

          <motion.div variants={item}>
            <Card title="Expense Ratio" style={{ height: '100%', textAlign: 'center' }}>
               <div style={{ marginTop: '30px', position: 'relative', display: 'inline-block' }}>
                 <svg viewBox="0 0 36 36" style={{ width: '180px', height: '180px', transform: 'rotate(-90deg)' }}>
                   <circle cx="18" cy="18" r="15.9" fill="transparent" stroke="var(--border-color)" strokeWidth="2.5" />
                   <motion.circle 
                     cx="18" cy="18" r="15.9" 
                     fill="transparent" 
                     stroke="var(--accent-pink)" 
                     strokeWidth="2.5" 
                     strokeDashoffset="100"
                     initial={{ strokeDasharray: "0 100" }}
                     animate={{ strokeDasharray: `${expensePercent} 100` }}
                     transition={{ duration: 1.5, ease: "easeOut" }}
                   />
                 </svg>
                 <div style={{ position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)' }}>
                   <h2 style={{ fontSize: '2.5rem', fontWeight: '700' }}>{expensePercent}%</h2>
                 </div>
               </div>
               <div style={{ marginTop: '24px' }}>
                 <p style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>Expenses vs. Total Income</p>
                 <p style={{ fontSize: '1.25rem', fontWeight: '700' }}>Health: {expensePercent < 80 ? 'Good' : 'Critical'}</p>
               </div>
            </Card>
          </motion.div>
        </div>

        {/* Transactions Section */}
        <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr', gap: '24px' }}>
          <motion.div variants={item}>
            <Card title="Recent Transactions" action={<Button variant="secondary" onClick={() => window.location.href='/transactions'}>Manage All</Button>}>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', marginTop: '16px' }}>
                {transactions.slice(0, 5).map((tx, idx) => (
                  <div key={idx} style={{ 
                    display: 'flex', 
                    alignItems: 'center', 
                    justifyContent: 'space-between', 
                    padding: '12px 16px', 
                    background: 'rgba(255, 255, 255, 0.02)', 
                    borderRadius: '12px',
                    border: '1px solid var(--border-color)'
                  }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                      <div style={{ width: '40px', height: '40px', borderRadius: '10px', background: tx.type === 'DEBIT' ? 'rgba(255,45,85,0.1)' : 'rgba(76,110,245,0.1)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: tx.type === 'DEBIT' ? 'var(--accent-pink)' : 'var(--accent-color)' }}>
                         {tx.type === 'DEBIT' ? <ArrowUpRight size={18} /> : <ArrowDownLeft size={18} />}
                      </div>
                      <div>
                        <p style={{ fontWeight: '600' }}>{tx.title}</p>
                        <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>{tx.categoryName}</p>
                      </div>
                    </div>
                    <p style={{ fontWeight: '700', color: tx.type === 'DEBIT' ? 'var(--accent-pink)' : 'var(--success)' }}>
                      {tx.type === 'DEBIT' ? '-' : '+'}${Math.abs(tx.amount).toLocaleString()}
                    </p>
                    <p style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>{tx.transactionDate}</p>
                  </div>
                ))}
              </div>
            </Card>
          </motion.div>

          <motion.div variants={item}>
            <Card title="Activity Feed">
              <div style={{ display: 'flex', flexDirection: 'column', gap: '24px', marginTop: '24px' }}>
                 <ActivityItem icon={ArrowUpRight} title="Spending is up 12% this week" time="2h ago" color="var(--accent-pink)" />
                 <ActivityItem icon={TrendingUp} title="Salary deposit confirmed" time="1 day ago" color="var(--success)" />
                 <ActivityItem icon={Mail} title="Statement for March is ready" time="2 days ago" color="var(--accent-color)" />
                 <ActivityItem icon={ShieldAlert} title="Account security updated" time="3 days ago" color="var(--accent-orange)" />
              </div>
            </Card>
          </motion.div>
        </div>
      </motion.div>
    </Layout>
  );
};

export default Dashboard;
