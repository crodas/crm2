import { useQuery } from '@tanstack/react-query'
import { useParams, Link } from 'react-router-dom'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function CustomerDetail() {
  const { t } = useTranslation()
  const { id } = useParams()
  const { data: customer } = useQuery({
    queryKey: ['customer', id],
    queryFn: () => api.get<any>(`/customers/${id}`),
  })
  const { data: timeline } = useQuery({
    queryKey: ['customer-timeline', id],
    queryFn: () => api.get<any[]>(`/customers/${id}/timeline`),
  })
  const { data: balance } = useQuery({
    queryKey: ['customer-balance', id],
    queryFn: () => api.get<any>(`/customers/${id}/balance`),
  })

  if (!customer) return <p>{t('common.loading')}</p>

  const typeLabel: Record<string, string> = {
    quote: t('customers.eventQuote'),
    sale: t('customers.eventSale'),
    booking: t('customers.eventBooking'),
    payment: t('customers.eventPayment'),
  }
  const typeLink: Record<string, string> = { quote: '/quotes', sale: '/sales', booking: '/bookings', payment: '/quotes' }
  const typeBadge: Record<string, string> = { quote: 'sent', sale: 'accepted', booking: 'scheduled', payment: 'completed' }

  return (
    <div>
      <h1>{customer.name}</h1>
      <div className="grid-2">
        <div className="card">
          <p><strong>{t('common.phone')}:</strong> {customer.phone || '—'}</p>
          <p><strong>{t('common.email')}:</strong> {customer.email || '—'}</p>
          <p><strong>{t('common.address')}:</strong> {customer.address || '—'}</p>
          <p><strong>{t('common.notes')}:</strong> {customer.notes || '—'}</p>
        </div>
        {balance && (
          <div className="card">
            <h2>{t('customers.balance')}</h2>
            <p><strong>{t('customers.totalOwed')}</strong> {balance.total_owed.toLocaleString()}</p>
            <p><strong>{t('customers.totalPaid')}</strong> {balance.total_paid.toLocaleString()}</p>
            <p><strong>{t('customers.outstanding')}</strong> <span style={{ color: balance.outstanding > 0 ? 'var(--status-danger)' : 'var(--status-success)' }}>
              {balance.outstanding.toLocaleString()}
            </span></p>
          </div>
        )}
      </div>

      <h2 className="mt-2">{t('customers.timeline')}</h2>
      <div className="card">
        {timeline && timeline.length > 0 ? timeline.map((e: any, i: number) => (
          <div key={i} className="timeline-item">
            <div className="timeline-type">
              <span className={`badge badge-${typeBadge[e.event_type] || 'draft'}`}>
                {typeLabel[e.event_type] || e.event_type}
              </span>
            </div>
            <div style={{ flex: 1 }}>
              <Link to={`${typeLink[e.event_type]}/${e.id}`}>{e.summary}</Link>
              {e.amount != null && <span style={{ marginLeft: '0.5rem', color: 'var(--text-muted)' }}>{e.amount.toLocaleString()}</span>}
            </div>
            <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem' }}>
              {new Date(e.date).toLocaleString()}
            </div>
          </div>
        )) : <p>{t('customers.noActivity')}</p>}
      </div>
    </div>
  )
}
