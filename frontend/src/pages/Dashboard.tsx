import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../api'

export default function Dashboard() {
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })
  const { data: products } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: stock } = useQuery({ queryKey: ['stock'], queryFn: () => api.get<any[]>('/inventory/stock') })
  const { data: quotes } = useQuery({ queryKey: ['quotes'], queryFn: () => api.get<any[]>('/quotes') })
  const { data: bookings } = useQuery({ queryKey: ['bookings'], queryFn: () => api.get<any[]>('/bookings') })

  return (
    <div>
      <h1>Dashboard</h1>
      <div className="dashboard-cards">
        <div className="card">
          <h2>Customers</h2>
          <p className="stat-number">{customers?.length ?? '...'}</p>
          <Link to="/customers/new" className="btn btn-primary btn-sm mt-1">Add Customer</Link>
        </div>
        <div className="card">
          <h2>Products</h2>
          <p className="stat-number">{products?.length ?? '...'}</p>
          <Link to="/products/new" className="btn btn-primary btn-sm mt-1">Add Product</Link>
        </div>
        <div className="card">
          <h2>Stock Items</h2>
          <p className="stat-number">{stock?.length ?? '...'}</p>
          <Link to="/inventory/receive" className="btn btn-primary btn-sm mt-1">Receive Inventory</Link>
        </div>
        <div className="card">
          <h2>Open Quotes</h2>
          <p className="stat-number">
            {quotes?.filter((q: any) => q.status !== 'booked').length ?? '...'}
          </p>
          <Link to="/quotes/new" className="btn btn-primary btn-sm mt-1">New Quote</Link>
        </div>
      </div>

      <h2 className="mt-2">Upcoming Bookings</h2>
      {bookings && bookings.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead>
              <tr><th>Title</th><th>Start</th><th>Status</th></tr>
            </thead>
            <tbody>
              {bookings.slice(0, 5).map((b: any) => (
                <tr key={b.id}>
                  <td><Link to={`/bookings/${b.id}`}>{b.title}</Link></td>
                  <td>{new Date(b.start_at).toLocaleDateString()}</td>
                  <td><span className={`badge badge-${b.status}`}>{b.status}</span></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="text-muted">No upcoming bookings</p>
      )}
    </div>
  )
}
