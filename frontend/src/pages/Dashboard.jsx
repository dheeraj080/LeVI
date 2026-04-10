import React, { useEffect, useState } from 'react';
import { Layout } from '../components/Layout';
import { Card, Button } from '../components/ui';
import { Plus, TrendingUp, TrendingDown, Wallet, ArrowUpRight, ArrowDownLeft } from 'lucide-react';
import api from '../api';
import { format } from 'date-fns';
import { motion } from 'framer-motion';

const Dashboard = () => {
  const [transactions, setTransactions] = useState([]);
  const [summary, setSummary] = useState({ totalBalance: 0, totalIncome: 0, totalExpense: 0 });
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchData = async () => {
      try {
        const [txRes, sumRes] = await Promise.all([
          api.get('/transactions'),
          api.get('/transactions/summary')
        ]);
        console.log('Dashboard TX:', txRes.data, 'Summary:', sumRes.data);
        setTransactions(txRes.data || []);
        setSummary(sumRes.data || { totalBalance: 0, totalIncome: 0, totalExpense: 0 });
      } catch (err) {
        console.error(err);
      } finally {
        setLoading(false);
      }
    };
    fetchData();
  }, []);

  const container = {
    hidden: { opacity: 0 },
    show: {
      opacity: 1,
      transition: {
        staggerChildren: 0.1
      }
    }
  };

  const item = {
    hidden: { y: 20, opacity: 0 },
    show: { y: 0, opacity: 1 }
  };

  return (
    <Layout title="Dashboard">
      {loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height: '50vh' }}>
          <div style={{ width: '40px', height: '40px', border: '3px solid var(--panel-color)', borderTopColor: 'var(--accent-color)', borderRadius: '50%', animation: 'spin 1s linear infinite' }} />
        </div>
      ) : (
        <motion.div 
          variants={container}
          initial="hidden"
          animate="show"
          style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))', gap: '24px' }}
        >
          {/* Stats Section */}
          <motion.div variants={item}>
            <Card style={{ background: 'linear-gradient(135deg, var(--accent-color), #4f46e5)', border: 'none' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                <div>
                  <p style={{ opacity: 0.8, fontSize: '0.9rem', marginBottom: '8px' }}>Total Balance</p>
                  <h2 style={{ fontSize: '2.5rem', fontWeight: '700' }}>${(summary.totalBalance || 0).toFixed(2)}</h2>
                </div>
                <div className="glass" style={{ padding: '10px' }}>
                  <Wallet size={24} />
                </div>
              </div>
              <div style={{ marginTop: '24px', display: 'flex', gap: '20px' }}>
                <div>
                  <p style={{ opacity: 0.8, fontSize: '0.75rem', display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <TrendingUp size={12} /> Income
                  </p>
                  <p style={{ fontWeight: '600' }}>+${(summary.totalIncome || 0).toFixed(2)}</p>
                </div>
                <div>
                  <p style={{ opacity: 0.8, fontSize: '0.75rem', display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <TrendingDown size={12} /> Expenses
                  </p>
                  <p style={{ fontWeight: '600' }}>-${(summary.totalExpense || 0).toFixed(2)}</p>
                </div>
              </div>
            </Card>
          </motion.div>
  
          {/* Transactions Section */}
          <motion.div variants={item} style={{ gridColumn: 'span 2' }}>
            <Card 
              title="Recent Transactions" 
              action={<Button variant="secondary" style={{ padding: '6px 12px', fontSize: '0.8rem' }}>View All</Button>}
            >
              <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
                {transactions.length === 0 ? (
                  <p style={{ textAlign: 'center', color: 'var(--text-secondary)', padding: '20px' }}>No transactions yet</p>
                ) : transactions.slice(0, 5).map((tx) => (
                  <div key={tx.id} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '12px', borderRadius: '12px', background: 'rgba(255, 255, 255, 0.02)', border: '1px solid var(--border-color)' }}>
                    <div style={{ display: 'flex', gap: '16px', alignItems: 'center' }}>
                      <div style={{ width: '40px', height: '40px', borderRadius: '10px', background: tx.type === 'DEBIT' ? 'rgba(239, 68, 68, 0.1)' : 'rgba(16, 185, 129, 0.1)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: tx.type === 'DEBIT' ? 'var(--danger)' : 'var(--success)' }}>
                        <div style={{ margin: 'auto' }}>
                          {tx.type === 'DEBIT' ? <ArrowUpRight size={20} /> : <ArrowDownLeft size={20} />}
                        </div>
                      </div>
                      <div>
                        <p style={{ fontWeight: '500' }}>{tx.title || tx.description}</p>
                        <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>{format(new Date(tx.transactionDate || tx.date), 'MMM d, yyyy')}</p>
                      </div>
                    </div>
                    <div style={{ textAlign: 'right' }}>
                      <p style={{ fontWeight: '600', color: tx.type === 'DEBIT' ? 'var(--danger)' : 'var(--success)' }}>
                        {tx.type === 'DEBIT' ? '-' : '+'}${Math.abs(tx.amount).toFixed(2)}
                      </p>
                      <p style={{ fontSize: '0.7rem', color: tx.categoryIcon || 'var(--text-secondary)' }}>{tx.categoryName || tx.category?.name || 'Uncategorized'}</p>
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          </motion.div>
  
          {/* Quick Actions */}
          <motion.div variants={item}>
            <Card title="Quick Actions">
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
                <Button style={{ height: '80px', flexDirection: 'column' }}>
                  <Plus size={20} />
                  <span>Add Expense</span>
                </Button>
                <Button variant="secondary" style={{ height: '80px', flexDirection: 'column' }}>
                  <TrendingUp size={20} />
                  <span>Add Income</span>
                </Button>
              </div>
            </Card>
          </motion.div>
        </motion.div>
      )}
    </Layout>
  );
};

export default Dashboard;
