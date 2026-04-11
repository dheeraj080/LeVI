import React, { useEffect, useState } from 'react';
import { Layout } from '../components/Layout';
import { Card, Button, Input } from '../components/ui';
import { Plus, Filter, Download, ArrowUpRight, ArrowDownLeft, Search, Trash2, Edit2 } from 'lucide-react';
import api from '../api';
import { format, startOfMonth, endOfMonth } from 'date-fns';
import { motion, AnimatePresence } from 'framer-motion';
import PeriodSelector from '../components/PeriodSelector';

const Transactions = () => {
  const [transactions, setTransactions] = useState([]);
  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);
  const [searchTerm, setSearchTerm] = useState('');
  const [showAddModal, setShowAddModal] = useState(false);
  const [editingTx, setEditingTx] = useState(null);
  const [currentPeriod, setCurrentPeriod] = useState(new Date());

  const [newTx, setNewTx] = useState({
    title: '',
    amount: '',
    transactionDate: format(new Date(), 'yyyy-MM-dd'),
    type: 'DEBIT',
    categoryId: ''
  });

  useEffect(() => {
    fetchData();
  }, [currentPeriod]);

  const fetchData = async () => {
    try {
      setLoading(true);
      const start = format(startOfMonth(currentPeriod), 'yyyy-MM-dd');
      const end = format(endOfMonth(currentPeriod), 'yyyy-MM-dd');
      
      const [txRes, catRes] = await Promise.all([
        api.get(`/transactions/range?start=${start}&end=${end}`),
        api.get('/categories')
      ]);
      console.log('Fetched Transactions:', txRes.data);
      console.log('Fetched Categories:', catRes.data);
      setTransactions(txRes.data || []);
      setCategories(catRes.data || []);
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  const handleAdd = async (e) => {
    e.preventDefault();
    try {
      const res = await api.post('/transactions', {
        ...newTx,
        amount: parseFloat(newTx.amount)
      });
      setTransactions([res.data, ...transactions]);
      setShowAddModal(false);
      setNewTx({ title: '', amount: '', transactionDate: format(new Date(), 'yyyy-MM-dd'), type: 'DEBIT', categoryId: '' });
    } catch (err) {
      console.error(err);
    }
  };

  const handleDelete = async (id) => {
    if (!window.confirm('Are you sure you want to delete this transaction?')) return;
    try {
      await api.delete(`/transactions/${id}`);
      setTransactions(transactions.filter(tx => tx.id !== id));
    } catch (err) {
      console.error(err);
      alert('Failed to delete transaction');
    }
  };

  const handleEdit = async (e) => {
    e.preventDefault();
    try {
      const res = await api.put(`/transactions/${editingTx.id}`, {
        ...newTx,
        amount: parseFloat(newTx.amount)
      });
      setTransactions(transactions.map(tx => tx.id === editingTx.id ? res.data : tx));
      setEditingTx(null);
      setNewTx({ title: '', amount: '', transactionDate: format(new Date(), 'yyyy-MM-dd'), type: 'DEBIT', categoryId: '' });
    } catch (err) {
      console.error(err);
      alert('Failed to update transaction');
    }
  };

  const startEdit = (tx) => {
    setEditingTx(tx);
    setNewTx({
      title: tx.title || tx.description || '',
      amount: Math.abs(tx.amount).toString(),
      transactionDate: format(new Date(tx.transactionDate || tx.date), 'yyyy-MM-dd'),
      type: tx.type || (tx.amount < 0 ? 'DEBIT' : 'CREDIT'),
      categoryId: tx.categoryId || tx.category?.id || ''
    });
  };

  const filtered = transactions.filter(tx =>
    (tx.title || tx.description || '').toLowerCase().includes(searchTerm.toLowerCase())
  );

  return (
    <Layout title="Transactions">
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '32px' }}>
        <div style={{ display: 'flex', gap: '16px', width: '400px' }}>
          <Input
            placeholder="Search transactions..."
            icon={Search}
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
          />
          <Button variant="secondary"><Filter size={18} /></Button>
        </div>
        <div style={{ display: 'flex', gap: '12px' }}>
          <PeriodSelector currentPeriod={currentPeriod} onChange={setCurrentPeriod} />
          <Button variant="secondary" style={{ gap: '8px' }}><Download size={18} /> Export</Button>
          <Button onClick={() => setShowAddModal(true)} style={{ gap: '8px' }}><Plus size={18} /> New Transaction</Button>
        </div>
      </div>

      <Card>
        <div style={{ overflowX: 'auto' }}>
          <table style={{ width: '100%', borderCollapse: 'collapse' }}>
            <thead>
              <tr style={{ textAlign: 'left', background: 'var(--panel-color)', borderBottom: '1px solid var(--border-color)', color: 'var(--text-secondary)', fontSize: '0.85rem' }}>
                <th style={{ padding: '16px', borderTopLeftRadius: '12px' }}>Date</th>
                <th style={{ padding: '16px' }}>Description</th>
                <th style={{ padding: '16px' }}>Category</th>
                <th style={{ padding: '16px', textAlign: 'right' }}>Amount</th>
                <th style={{ padding: '16px', textAlign: 'right', borderTopRightRadius: '12px' }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((tx, index) => (
                <motion.tr
                  initial={{ opacity: 0, x: -10 }}
                  animate={{ opacity: 1, x: 0 }}
                  transition={{ delay: index * 0.05 }}
                  key={tx.id}
                  style={{ borderBottom: '1px solid var(--border-color)', fontSize: '0.95rem' }}
                >
                  <td style={{ padding: '16px', color: 'var(--text-secondary)' }}>{format(new Date(tx.transactionDate || tx.date), 'MMM d, yyyy')}</td>
                  <td style={{ padding: '16px' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                      <div style={{ color: tx.type === 'DEBIT' ? 'var(--danger)' : 'var(--success)' }}>
                        {tx.type === 'DEBIT' ? <ArrowUpRight size={16} /> : <ArrowDownLeft size={16} />}
                      </div>
                      {tx.title || tx.description}
                    </div>
                  </td>
                  <td style={{ padding: '16px' }}>
                    <span style={{
                      padding: '4px 10px',
                      borderRadius: '20px',
                      background: 'rgba(255, 255, 255, 0.05)',
                      fontSize: '0.75rem',
                      borderLeft: `3px solid ${tx.categoryIcon || '#ccc'}`
                    }}>
                      {tx.categoryName || tx.category?.name || 'Uncategorized'}
                    </span>
                  </td>
                  <td style={{ padding: '16px', textAlign: 'right', fontWeight: '600', color: tx.type === 'DEBIT' ? 'white' : 'var(--success)' }}>
                    {tx.type === 'DEBIT' ? '-' : '+'}${Math.abs(tx.amount).toFixed(2)}
                  </td>
                  <td style={{ padding: '16px', textAlign: 'right' }}>
                    <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
                      <button 
                        onClick={() => startEdit(tx)}
                        style={{ background: 'transparent', border: 'none', color: 'var(--text-secondary)', cursor: 'pointer', padding: '4px' }}
                      >
                        <Edit2 size={16} />
                      </button>
                      <button 
                        onClick={() => handleDelete(tx.id)}
                        style={{ background: 'transparent', border: 'none', color: 'var(--danger)', cursor: 'pointer', padding: '4px' }}
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </td>
                </motion.tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>

      {/* Add/Edit Transaction Modal */}
      <AnimatePresence>
        {(showAddModal || editingTx) && (
          <div style={{ position: 'fixed', top: 0, left: 0, right: 0, bottom: 0, background: 'rgba(0,0,0,0.8)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 100 }}>
            <motion.div
              initial={{ scale: 0.9, opacity: 0 }}
              animate={{ scale: 1, opacity: 1 }}
              exit={{ scale: 0.9, opacity: 0 }}
              style={{ width: '100%', maxWidth: '500px' }}
            >
              <Card title={editingTx ? "Edit Transaction" : "New Transaction"}>
                <form onSubmit={editingTx ? handleEdit : handleAdd} style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
                  <Input
                    label="Title"
                    required
                    value={newTx.title}
                    onChange={(e) => setNewTx({ ...newTx, title: e.target.value })}
                  />
                  <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '16px' }}>
                    <Input
                      label="Amount ($)"
                      type="number"
                      step="0.01"
                      required
                      value={newTx.amount}
                      onChange={(e) => setNewTx({ ...newTx, amount: e.target.value })}
                    />
                    <Input
                      label="Date"
                      type="date"
                      required
                      value={newTx.transactionDate}
                      onChange={(e) => setNewTx({ ...newTx, transactionDate: e.target.value })}
                    />
                  </div>
                  <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '16px' }}>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                      <label style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>Type</label>
                      <select
                        style={{
                          background: 'var(--glass-bg)',
                          border: '1px solid var(--border-color)',
                          borderRadius: '8px',
                          padding: '12px',
                          color: 'var(--text-primary)',
                          outline: 'none',
                          appearance: 'none',
                          WebkitAppearance: 'none'
                        }}
                        value={newTx.type}
                        onChange={(e) => setNewTx({ ...newTx, type: e.target.value })}
                      >
                        <option value="DEBIT" style={{ background: 'var(--panel-color)', color: 'var(--text-primary)' }}>Expense</option>
                        <option value="CREDIT" style={{ background: 'var(--panel-color)', color: 'var(--text-primary)' }}>Income</option>
                      </select>
                    </div>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                      <label style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>Category</label>
                      <select
                        style={{
                          background: 'var(--glass-bg)',
                          border: '1px solid var(--border-color)',
                          borderRadius: '8px',
                          padding: '12px',
                          color: 'var(--text-primary)',
                          outline: 'none',
                          appearance: 'none',
                          WebkitAppearance: 'none'
                        }}
                        value={newTx.categoryId}
                        onChange={(e) => setNewTx({ ...newTx, categoryId: e.target.value })}
                        required
                      >
                        <option value="" style={{ background: 'var(--panel-color)', color: 'var(--text-primary)' }}>Select Category</option>
                        {categories.map(c => (
                          <option key={c.id} value={c.id} style={{ background: 'var(--panel-color)', color: 'var(--text-primary)' }}>
                            {c.name}
                          </option>
                        ))}
                      </select>
                    </div>
                  </div>
                  <div style={{ display: 'flex', gap: '12px', marginTop: '12px' }}>
                    <Button type="button" variant="secondary" onClick={() => { setShowAddModal(false); setEditingTx(null); }} style={{ flex: 1 }}>Cancel</Button>
                    <Button type="submit" style={{ flex: 1 }}>{editingTx ? 'Update' : 'Save'} Transaction</Button>
                  </div>
                </form>
              </Card>
            </motion.div>
          </div>
        )}
      </AnimatePresence>
    </Layout>
  );
};

export default Transactions;
