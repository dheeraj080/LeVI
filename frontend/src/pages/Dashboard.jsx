import React, { useEffect, useState } from 'react';
import { Layout } from '../components/Layout';
import { Card, Button } from '../components/ui';
import { 
  Wallet, 
  TrendingUp, 
  TrendingDown, 
  ArrowUpRight, 
  ArrowDownLeft,
  Mail,
  ShieldAlert,
  CreditCard
} from 'lucide-react';
import api from '../api';
import { motion } from 'framer-motion';
import { 
  AreaChart, 
  Area, 
  XAxis, 
  YAxis, 
  CartesianGrid, 
  Tooltip, 
  ResponsiveContainer,
  PieChart as RePieChart,
  Pie,
  Cell
} from 'recharts';

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
  const [chartData, setChartData] = useState([]);
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
      const txs = txRes.data || [];
      setTransactions(txs);
      setSummary(sumRes.data || { totalBalance: 0, totalIncome: 0, totalExpense: 0 });
      
      // Process data for Recharts
      const last7Days = [...Array(7)].map((_, i) => {
        const d = new Date();
        d.setDate(d.getDate() - (6 - i));
        return d.toISOString().split('T')[0];
      });

      const processedData = last7Days.map(date => {
        const dayTxs = txs.filter(t => t.transactionDate === date);
        return {
          name: new Date(date).toLocaleDateString(undefined, { weekday: 'short' }),
          income: dayTxs.filter(t => t.type === 'CREDIT').reduce((sum, t) => sum + t.amount, 0),
          expense: dayTxs.filter(t => t.type === 'DEBIT').reduce((sum, t) => sum + Math.abs(t.amount), 0),
        };
      });
      setChartData(processedData);

    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  const expensePercent = summary.totalIncome > 0 
    ? Math.min(Math.round((summary.totalExpense / summary.totalIncome) * 100), 100)
    : 0;

  const donutData = [
    { name: 'Expenses', value: summary.totalExpense, color: 'var(--danger)' },
    { name: 'Remaining', value: Math.max(0, summary.totalIncome - summary.totalExpense), color: 'var(--border-color)' }
  ];

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
            <Card title="Activity Flow">
              <div style={{ height: '320px', width: '100%', marginTop: '24px' }}>
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart data={chartData}>
                    <defs>
                      <linearGradient id="colorInc" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stopColor="var(--success)" stopOpacity={0.3}/>
                        <stop offset="95%" stopColor="var(--success)" stopOpacity={0}/>
                      </linearGradient>
                      <linearGradient id="colorExp" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stopColor="var(--danger)" stopOpacity={0.3}/>
                        <stop offset="95%" stopColor="var(--danger)" stopOpacity={0}/>
                      </linearGradient>
                    </defs>
                    <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="var(--border-color)" />
                    <XAxis 
                      dataKey="name" 
                      axisLine={false} 
                      tickLine={false} 
                      tick={{ fill: 'var(--text-secondary)', fontSize: 12 }} 
                      dy={10}
                    />
                    <YAxis 
                      axisLine={false} 
                      tickLine={false} 
                      tick={{ fill: 'var(--text-secondary)', fontSize: 12 }} 
                    />
                    <Tooltip 
                      contentStyle={{ 
                        backgroundColor: 'var(--panel-color)', 
                        border: '1px solid var(--border-color)',
                        borderRadius: '12px',
                        boxShadow: 'var(--shadow-lg)'
                      }}
                      itemStyle={{ fontSize: '13px', fontWeight: '600' }}
                    />
                    <Area type="monotone" dataKey="income" stroke="var(--success)" strokeWidth={2} fillOpacity={1} fill="url(#colorInc)" />
                    <Area type="monotone" dataKey="expense" stroke="var(--danger)" strokeWidth={2} fillOpacity={1} fill="url(#colorExp)" />
                  </AreaChart>
                </ResponsiveContainer>
              </div>
            </Card>
          </motion.div>

          <motion.div variants={item}>
            <Card title="Resource Allocation" style={{ height: '100%' }}>
               <div style={{ height: '240px', width: '100%', position: 'relative' }}>
                <ResponsiveContainer width="100%" height="100%">
                  <RePieChart>
                    <Pie
                      data={donutData}
                      cx="50%"
                      cy="50%"
                      innerRadius={60}
                      outerRadius={80}
                      paddingAngle={5}
                      dataKey="value"
                      startAngle={90}
                      endAngle={450}
                    >
                      {donutData.map((entry, index) => (
                        <Cell key={`cell-${index}`} fill={entry.color} stroke="none" />
                      ))}
                    </Pie>
                  </RePieChart>
                </ResponsiveContainer>
                <div style={{ position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)', textAlign: 'center' }}>
                  <p style={{ fontSize: '2rem', fontWeight: '800', lineHeight: 1 }}>{expensePercent}%</p>
                  <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)', marginTop: '4px' }}>Utilized</p>
                </div>
               </div>
               <div style={{ marginTop: '24px', textAlign: 'center' }}>
                 <p style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>Status</p>
                 <p style={{ fontSize: '1.2rem', fontWeight: '700', color: expensePercent < 80 ? 'var(--success)' : 'var(--danger)' }}>
                   {expensePercent < 80 ? 'Excellent' : 'At Risk'}
                 </p>
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
                    background: 'var(--glass-bg)', 
                    borderRadius: '12px',
                    border: '1px solid var(--border-color)'
                  }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                      <div style={{ 
                        width: '40px', 
                        height: '40px', 
                        borderRadius: '10px', 
                        background: tx.type === 'DEBIT' ? 'rgba(255,48,78,0.1)' : 'rgba(48,209,88,0.1)', 
                        display: 'flex', 
                        alignItems: 'center', 
                        justifyContent: 'center', 
                        color: tx.type === 'DEBIT' ? 'var(--danger)' : 'var(--success)' 
                      }}>
                         {tx.type === 'DEBIT' ? <ArrowUpRight size={18} /> : <ArrowDownLeft size={18} />}
                      </div>
                      <div>
                        <p style={{ fontWeight: '600' }}>{tx.title}</p>
                        <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>{tx.categoryName}</p>
                      </div>
                    </div>
                    <div>
                      <p style={{ fontWeight: '700', color: tx.type === 'DEBIT' ? 'var(--danger)' : 'var(--success)', textAlign: 'right' }}>
                        {tx.type === 'DEBIT' ? '-' : '+'}${Math.abs(tx.amount).toLocaleString()}
                      </p>
                      <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)', textAlign: 'right' }}>{tx.transactionDate}</p>
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          </motion.div>

          <motion.div variants={item}>
            <Card title="Activity Feed">
              <div style={{ display: 'flex', flexDirection: 'column', gap: '24px', marginTop: '24px' }}>
                 <ActivityItem icon={ArrowUpRight} title="Spending is up 12% this week" time="2h ago" color="var(--danger)" />
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
