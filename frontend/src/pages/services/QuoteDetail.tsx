import { useState } from 'react'
import { useParams, Link } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function QuoteDetail() {
  const { t } = useTranslation()
  const { id } = useParams()
  const qc = useQueryClient()

  const { data } = useQuery({
    queryKey: ['quote', id],
    queryFn: () => api.get<any>(`/quotes/${id}`),
  })
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })

  const [payAmount, setPayAmount] = useState(0)
  const [payMethod, setPayMethod] = useState('cash')
  const [payNotes, setPayNotes] = useState('')

  const statusMutation = useMutation({
    mutationFn: (status: string) => api.patch(`/quotes/${id}/status`, { status }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['quote', id] }),
  })

  const payMutation = useMutation({
    mutationFn: () => api.post(`/quotes/${id}/payments`, {
      amount: payAmount,
      method: payMethod,
      notes: payNotes || null,
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['quote', id] })
      setPayAmount(0)
      setPayNotes('')
    },
  })

  if (!data) return <p>{t('common.loading')}</p>

  const { quote, lines, payments, total_paid, balance, bookings } = data
  const customer = customers?.find((c: any) => c.id === quote.customer_id)
  const statuses = ['draft', 'sent', 'follow_up', 'accepted', 'booked']

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>
          {t('quotes.quoteNumber', { id: quote.id })} {quote.title}
          {quote.is_debt ? <span className="badge badge-follow_up" style={{ marginLeft: '0.5rem' }}>{t('quotes.debt')}</span> : null}
        </h1>
        <span className={`badge badge-${quote.status}`}>{quote.status}</span>
      </div>

      <div className="grid-2">
        <div className="card">
          <h2>{t('common.details')}</h2>
          <p><strong>{t('sales.customer_label')}</strong> {customer?.name ?? quote.customer_id}</p>
          <p><strong>{t('quotes.description_label')}</strong> {quote.description || '—'}</p>
          <p><strong>{t('quotes.total_label')}</strong> {quote.total_amount.toLocaleString()}</p>
          <p><strong>{t('quotes.paid_label')}</strong> {total_paid.toLocaleString()}</p>
          <p><strong>{t('quotes.balance_label')}</strong> <span style={{ color: balance > 0 ? 'var(--status-danger)' : 'var(--status-success)' }}>
            {balance.toLocaleString()}
          </span></p>
          <p><strong>{t('quotes.created_label')}</strong> {new Date(quote.created_at).toLocaleDateString()}</p>
          <p><strong>{t('common.versionId')}</strong> <code title={quote.version_id}>{quote.version_id?.slice(0, 8)}</code></p>

          <h2 className="mt-2">{t('quotes.status')}</h2>
          <div className="flex gap-1">
            {statuses.map(s => (
              <button
                key={s}
                className={`btn btn-sm ${s === quote.status ? 'btn-primary' : ''}`}
                onClick={() => statusMutation.mutate(s)}
                disabled={s === quote.status}
              >
                {s}
              </button>
            ))}
          </div>
        </div>

        <div className="card">
          <h2>{t('quotes.recordPayment')}</h2>
          <div className="form-group">
            <label>{t('common.amount')}</label>
            <input type="number" value={payAmount} onChange={e => setPayAmount(Number(e.target.value))} />
          </div>
          <div className="form-group">
            <label>{t('common.method')}</label>
            <select value={payMethod} onChange={e => setPayMethod(e.target.value)}>
              <option value="cash">{t('quotes.cash')}</option>
              <option value="card">{t('quotes.card')}</option>
              <option value="transfer">{t('quotes.transfer')}</option>
              <option value="check">{t('quotes.check')}</option>
            </select>
          </div>
          <div className="form-group">
            <label>{t('common.notes')}</label>
            <input value={payNotes} onChange={e => setPayNotes(e.target.value)} />
          </div>
          <button
            className="btn btn-primary"
            onClick={() => payMutation.mutate()}
            disabled={payAmount <= 0 || payMutation.isPending}
          >
            {payMutation.isPending ? t('common.recording') : t('quotes.recordPayment')}
          </button>
        </div>
      </div>

      <h2 className="mt-2">{t('quotes.lineItems')}</h2>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t('common.versionId')}</th><th>{t('common.description')}</th><th>{t('quotes.qty')}</th><th>{t('quotes.unitPrice')}</th><th>{t('common.total')}</th></tr></thead>
          <tbody>
            {lines.map((l: any) => (
              <tr key={l.id}>
                <td><code title={l.version_id}>{l.version_id?.slice(0, 8)}</code></td>
                <td>{l.description}</td>
                <td>{l.quantity}</td>
                <td>{l.unit_price.toLocaleString()}</td>
                <td>{(l.quantity * l.unit_price).toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <h2 className="mt-2">{t('quotes.paymentHistory')}</h2>
      {payments.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead><tr><th>{t('common.date')}</th><th>{t('common.amount')}</th><th>{t('common.method')}</th><th>{t('common.notes')}</th></tr></thead>
            <tbody>
              {payments.map((p: any) => (
                <tr key={p.id}>
                  <td>{new Date(p.paid_at).toLocaleDateString()}</td>
                  <td>{p.amount.toLocaleString()}</td>
                  <td>{p.method}</td>
                  <td>{p.notes || '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : <p>{t('quotes.noPayments')}</p>}

      <h2 className="mt-2">{t('bookings.linkedBookings')}</h2>
      {bookings && bookings.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead><tr><th>{t('common.id')}</th><th>{t('common.versionId')}</th><th>{t('common.title')}</th><th>{t('common.description')}</th><th>{t('common.location')}</th><th>{t('bookings.start')}</th><th>{t('common.status')}</th></tr></thead>
            <tbody>
              {bookings.map((b: any) => (
                <tr key={b.id}>
                  <td><Link to={`/bookings/${b.id}`}>#{b.id}</Link></td>
                  <td><code title={b.version_id}>{b.version_id?.slice(0, 8)}</code></td>
                  <td>{b.title}</td>
                  <td>{b.description || '—'}</td>
                  <td>{b.location || '—'}</td>
                  <td>{new Date(b.start_at).toLocaleString()}</td>
                  <td><span className={`badge badge-${b.status}`}>{b.status}</span></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : <p>{t('bookings.noLinkedBookings')}</p>}


    </div>
  )
}
