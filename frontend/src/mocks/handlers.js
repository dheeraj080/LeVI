import { http, HttpResponse } from 'msw';

const mockTransactions = [
  { id: '1', amount: 150.50, description: 'Grocery Store', date: '2026-04-10', category: { name: 'Food', color: '#10b981' } },
  { id: '2', amount: 1200.00, description: 'Rent', date: '2026-04-01', category: { name: 'Housing', color: '#6366f1' } },
  { id: '3', amount: 45.00, description: 'Coffee Shop', date: '2026-04-09', category: { name: 'Food', color: '#10b981' } },
  { id: '4', amount: 80.00, description: 'Gas Station', date: '2026-04-08', category: { name: 'Transport', color: '#f59e0b' } },
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
      { id: 'c1', name: 'Food', color: '#10b981' },
      { id: 'c2', name: 'Housing', color: '#6366f1' },
      { id: 'c3', name: 'Transport', color: '#f59e0b' },
    ]);
  }),

  http.get('/api/v1/transactions/total-expense', () => {
    return HttpResponse.json(1475.50);
  }),
];
