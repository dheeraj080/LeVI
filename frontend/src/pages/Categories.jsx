import React, { useEffect, useState } from 'react';
import { Layout } from '../components/Layout';
import { Card, Button, Input } from '../components/ui';
import { Plus, Trash2, Edit2, Palette } from 'lucide-react';
import api from '../api';
import { motion } from 'framer-motion';

const Categories = () => {
  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);
  const [newCat, setNewCat] = useState({ name: '', icon: '#6366f1', type: 'DEBIT' });
  const [editingCat, setEditingCat] = useState(null);

  useEffect(() => {
    fetchCategories();
  }, []);

  const fetchCategories = async () => {
    try {
      const res = await api.get('/categories');
      setCategories(res.data);
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  const handleCreate = async () => {
    if (!newCat.name) return;
    try {
      const res = await api.post('/categories', newCat);
      setCategories([...categories, res.data]);
      setNewCat({ name: '', icon: '#6366f1', type: 'DEBIT' });
    } catch (err) {
      console.error(err);
    }
  };

  const startEdit = (cat) => {
    setEditingCat(cat);
    setNewCat({ name: cat.name, icon: cat.icon || '#6366f1', type: cat.type || 'DEBIT' });
  };

  const handleUpdate = async () => {
    try {
      const res = await api.put(`/categories/${editingCat.id}`, newCat);
      setCategories(categories.map(c => c.id === editingCat.id ? res.data : c));
      setEditingCat(null);
      setNewCat({ name: '', icon: '#6366f1', type: 'DEBIT' });
    } catch (err) {
      console.error(err);
    }
  };

  const cancelEdit = () => {
    setEditingCat(null);
    setNewCat({ name: '', icon: '#6366f1', type: 'DEBIT' });
  };

  const handleDelete = async (id) => {
    try {
      await api.delete(`/categories/${id}`);
      setCategories(categories.filter(c => c.id !== id));
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <Layout title="Categories">
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 350px', gap: '32px' }}>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: '20px' }}>
          {categories.map((cat) => (
            <motion.div 
              layout
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              key={cat.id}
            >
              <Card style={{ borderTop: `4px solid ${cat.icon || '#6366f1'}` }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <div>
                    <h4 style={{ fontWeight: '600' }}>{cat.name}</h4>
                    <p style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>0 Transactions</p>
                  </div>
                  <div style={{ display: 'flex', gap: '8px' }}>
                    <button 
                      onClick={() => startEdit(cat)}
                      style={{ color: 'var(--text-secondary)', background: 'transparent', cursor: 'pointer', border: 'none' }}
                    ><Edit2 size={16} /></button>
                    <button 
                      onClick={() => handleDelete(cat.id)}
                      style={{ color: 'var(--danger)', background: 'transparent', cursor: 'pointer', border: 'none' }}
                    ><Trash2 size={16} /></button>
                  </div>
                </div>
              </Card>
            </motion.div>
          ))}
        </div>

        <div>
          <Card title={editingCat ? "Edit Category" : "Add New Category"}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
              <Input 
                label="Category Name" 
                placeholder="e.g. Shopping" 
                value={newCat.name}
                onChange={(e) => setNewCat({ ...newCat, name: e.target.value })}
              />
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                <label style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>Type</label>
                <select 
                  style={{ 
                    background: 'var(--glass-bg)', border: '1px solid var(--border-color)', borderRadius: '8px', padding: '12px', color: 'var(--text-primary)', outline: 'none', appearance: 'none', WebkitAppearance: 'none'
                  }}
                  value={newCat.type}
                  onChange={(e) => setNewCat({ ...newCat, type: e.target.value })}
                >
                    <option value="DEBIT" style={{ background: 'var(--panel-color)', color: 'var(--text-primary)' }}>Expense</option>
                    <option value="CREDIT" style={{ background: 'var(--panel-color)', color: 'var(--text-primary)' }}>Income</option>
                </select>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                <label style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>Label Color</label>
                <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
                  {['var(--accent-color)', 'var(--success)', 'var(--accent-pink)', 'var(--accent-orange)', 'var(--accent-purple)', 'var(--warning)'].map(icon => (
                    <button 
                      key={icon}
                      onClick={() => setNewCat({ ...newCat, icon })}
                      style={{ 
                        width: '32px', 
                        height: '32px', 
                        borderRadius: '8px', 
                        background: icon,
                        border: newCat.icon === icon ? '2px solid white' : 'none',
                        cursor: 'pointer'
                      }}
                    />
                  ))}
                  <div style={{ position: 'relative' }}>
                    <input 
                      type="color" 
                      value={newCat.icon}
                      onChange={(e) => setNewCat({ ...newCat, icon: e.target.value })}
                      style={{ position: 'absolute', opacity: 0, width: '100%', height: '100%', cursor: 'pointer' }}
                    />
                    <div style={{ width: '32px', height: '32px', borderRadius: '8px', background: 'var(--panel-color)', border: '1px solid var(--border-color)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                      <Palette size={16} />
                    </div>
                  </div>
                </div>
              </div>
              <div style={{ display: 'flex', gap: '12px' }}>
                {editingCat && (
                  <Button variant="secondary" style={{ flex: 1 }} onClick={cancelEdit}>
                    Cancel
                  </Button>
                )}
                <Button style={{ flex: 1 }} onClick={editingCat ? handleUpdate : handleCreate}>
                  <Plus size={18} /> {editingCat ? 'Update Category' : 'Create Category'}
                </Button>
              </div>
            </div>
          </Card>
        </div>
      </div>
    </Layout>
  );
};

export default Categories;
