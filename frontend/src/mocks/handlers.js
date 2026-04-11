import { http, HttpResponse } from 'msw';

const mockTransactions = [
  { 
    id: '1', 
    title: 'Grocery Store', 
    amount: 150.50, 
    transactionDate: '2026-04-10', 
    type: 'DEBIT',
    categoryName: 'Food', 
    categoryIcon: '#10b981' 
  },
  { 
    id: '2', 
    title: 'Rent', 
    amount: 1200.00, 
    transactionDate: '2026-04-01', 
    type: 'DEBIT',
    categoryName: 'Housing', 
    categoryIcon: '#6366f1' 
  },
  { 
    id: '3', 
    title: 'Salary', 
    amount: 3500.00, 
    transactionDate: '2026-04-05', 
    type: 'CREDIT',
    categoryName: 'Income', 
    categoryIcon: '#10b981' 
  },
  { 
    id: '4', 
    title: 'Coffee Shop', 
    amount: 45.00, 
    transactionDate: '2026-04-09', 
    type: 'DEBIT',
    categoryName: 'Food', 
    categoryIcon: '#10b981' 
  },
];

export const handlers = [
  http.post('/api/v1/auth/login', async ({ request }) => {
    const { email } = await request.json();
    return HttpResponse.json({
      accessToken: 'mock-access-token',
      refreshToken: 'mock-refresh-token',
      accessTtlSeconds: 3600,
      user: {
        id: 'user-1',
        email: email,
        name: 'Mock User'
      }
    });
  }),

  http.get('/api/v1/transactions', () => {
    return HttpResponse.json(mockTransactions);
  }),

  http.get('/api/v1/categories', () => {
    return HttpResponse.json([
      { id: 'c1', name: 'Food', icon: '#10b981', type: 'DEBIT' },
      { id: 'c2', name: 'Housing', icon: '#6366f1', type: 'DEBIT' },
      { id: 'c3', name: 'Transport', icon: '#f59e0b', type: 'DEBIT' },
      { id: 'c4', name: 'Salary', icon: '#10b981', type: 'CREDIT' },
    ]);
  }),

  http.get('/api/v1/transactions/summary', () => {
    return HttpResponse.json({
      totalBalance: 2104.50,
      totalIncome: 3500.00,
      totalExpense: 1395.50
    });
  }),

  http.get('/api/v1/transactions/total-expense', () => {
    return HttpResponse.json(1395.50);
  }),
];
