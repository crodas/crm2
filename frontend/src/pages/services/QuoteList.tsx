import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'

export default function QuoteList() {
  const { data: quotes } = useQuery({
    queryKey: ['quotes'],
    queryFn: () => api.get<any[]>('/quotes'),
  })

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>Quotes</h1>
        <div className="flex gap-1">
          <Link to="/debts/new" className="btn">Quick Debt</Link>
          <Link to="/quotes/new" className="btn btn-primary">New Quote</Link>
        </div>
      </div>
      <table>
        <thead>
          <tr><th>ID</th><th>Title</th><th>Status</th><th>Amount</th><th>Date</th></tr>
        </thead>
        <tbody>
          {quotes?.map((q: any) => (
            <tr key={q.id}>
              <td><Link to={`/quotes/${q.id}`}>#{q.id}</Link></td>
              <td>
                {q.title}
                {q.is_debt ? <span className="badge badge-follow_up" style={{ marginLeft: '0.5rem' }}>debt</span> : null}
              </td>
              <td><span className={`badge badge-${q.status}`}>{q.status}</span></td>
              <td>{q.total_amount.toLocaleString()}</td>
              <td>{new Date(q.created_at).toLocaleDateString()}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
