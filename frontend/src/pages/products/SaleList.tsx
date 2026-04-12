import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'

export default function SaleList() {
  const { data: sales } = useQuery({
    queryKey: ['sales'],
    queryFn: () => api.get<any[]>('/sales'),
  })

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>Sales</h1>
        <Link to="/sales/new" className="btn btn-primary">New Sale</Link>
      </div>
      <table>
        <thead>
          <tr><th>ID</th><th>Date</th><th>Total</th><th>Notes</th></tr>
        </thead>
        <tbody>
          {sales?.map((s: any) => (
            <tr key={s.id}>
              <td><Link to={`/sales/${s.id}`}>#{s.id}</Link></td>
              <td>{new Date(s.sold_at).toLocaleDateString()}</td>
              <td><strong>{s.total_amount.toLocaleString()}</strong></td>
              <td>{s.notes || '—'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
