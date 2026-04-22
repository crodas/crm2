import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../api'
import { useTranslation } from '../i18n'

export default function Dashboard() {
  const { t } = useTranslation()
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })
  const { data: products } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: stock } = useQuery({ queryKey: ['stock'], queryFn: () => api.get<any[]>('/inventory/stock') })
  const { data: quotes } = useQuery({ queryKey: ['quotes'], queryFn: () => api.get<any[]>('/quotes') })
  const { data: bookings } = useQuery({ queryKey: ['bookings'], queryFn: () => api.get<any[]>('/bookings') })
  const { data: supplierBalance } = useQuery({ queryKey: ['supplier-balance'], queryFn: () => api.get<any>('/supplier-balance') })
  const { data: receivables } = useQuery({ queryKey: ['receivables'], queryFn: () => api.get<any>('/receivables') })

  return (
    <div>
      <h1>{t('dashboard.title')}</h1>
      <div className="dashboard-cards">
        <div className="card">
          <h2>{t('dashboard.customers')}</h2>
          <p className="stat-number">{customers?.length ?? '...'}</p>
          <Link to="/customers/new" className="btn btn-primary btn-sm mt-1">{t('dashboard.addCustomer')}</Link>
        </div>
        <div className="card">
          <h2>{t('dashboard.products')}</h2>
          <p className="stat-number">{products?.length ?? '...'}</p>
          <Link to="/products/new" className="btn btn-primary btn-sm mt-1">{t('dashboard.addProduct')}</Link>
        </div>
        <div className="card">
          <h2>{t('dashboard.stockItems')}</h2>
          <p className="stat-number">{stock?.length ?? '...'}</p>
          <Link to="/inventory/receive" className="btn btn-primary btn-sm mt-1">{t('dashboard.receiveInventory')}</Link>
        </div>
        <div className="card">
          <h2>{t('dashboard.openQuotes')}</h2>
          <p className="stat-number">
            {quotes?.filter((q: any) => q.status !== 'booked').length ?? '...'}
          </p>
          <Link to="/quotes/new" className="btn btn-primary btn-sm mt-1">{t('dashboard.newQuote')}</Link>
        </div>
        <div className="card">
          <h2>{t('dashboard.toCollect')}</h2>
          <p className="stat-number" style={{ color: receivables?.outstanding > 0 ? 'var(--status-success)' : undefined }}>
            {receivables?.outstanding?.toLocaleString() ?? '...'}
          </p>
        </div>
        <div className="card">
          <h2>{t('dashboard.toPay')}</h2>
          <p className="stat-number" style={{ color: supplierBalance?.outstanding > 0 ? 'var(--status-danger)' : 'var(--status-success)' }}>
            {supplierBalance?.outstanding?.toLocaleString() ?? '...'}
          </p>
        </div>
      </div>

      <h2 className="mt-2">{t('dashboard.upcomingBookings')}</h2>
      {bookings && bookings.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead>
              <tr><th>{t('common.title')}</th><th>{t('dashboard.start')}</th><th>{t('common.status')}</th></tr>
            </thead>
            <tbody>
              {bookings.slice(0, 5).map((b: any) => (
                <tr key={b.id}>
                  <td><Link to={`/bookings/${b.id}`}>{b.title}</Link></td>
                  <td>{new Date(b.start_at).toLocaleString()}</td>
                  <td><span className={`badge badge-${b.status}`}>{b.status}</span></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="text-muted">{t('dashboard.noUpcomingBookings')}</p>
      )}
    </div>
  )
}
