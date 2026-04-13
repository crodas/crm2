import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function BookingDetail() {
  const { t } = useTranslation()
  const { id } = useParams()
  const qc = useQueryClient()

  const { data } = useQuery({
    queryKey: ['booking', id],
    queryFn: () => api.get<any>(`/bookings/${id}`),
  })

  const [woDesc, setWoDesc] = useState('')
  const [woLocation, setWoLocation] = useState('')

  const woMutation = useMutation({
    mutationFn: () => api.post(`/bookings/${id}/work-orders`, {
      description: woDesc,
      location: woLocation || null,
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['booking', id] })
      setWoDesc('')
      setWoLocation('')
    },
  })

  if (!data) return <p>{t('common.loading')}</p>

  const { booking, work_orders, quotes } = data

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>{t('bookings.bookingTitle')} {booking.title}</h1>
        <span className={`badge badge-${booking.status}`}>{booking.status}</span>
      </div>

      <div className="grid-2">
        <div className="card">
          <h2>{t('common.details')}</h2>
          <p><strong>{t('bookings.start_label')}</strong> {new Date(booking.start_at).toLocaleString()}</p>
          <p><strong>{t('bookings.end_label')}</strong> {new Date(booking.end_at).toLocaleString()}</p>
          <p><strong>{t('bookings.notes_label')}</strong> {booking.notes || '—'}</p>
        </div>
        <div className="card">
          <h2>{t('bookings.addWorkOrder')}</h2>
          <div className="form-group">
            <label>{t('common.description')}</label>
            <textarea value={woDesc} onChange={e => setWoDesc(e.target.value)} rows={2} />
          </div>
          <div className="form-group">
            <label>{t('common.location')}</label>
            <input value={woLocation} onChange={e => setWoLocation(e.target.value)} />
          </div>
          <button
            className="btn btn-primary"
            onClick={() => woMutation.mutate()}
            disabled={!woDesc || woMutation.isPending}
          >
            {t('bookings.addWorkOrder')}
          </button>
        </div>
      </div>

      <h2 className="mt-2">{t('bookings.workOrders')}</h2>
      {work_orders.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead><tr><th>{t('common.id')}</th><th>{t('common.description')}</th><th>{t('common.location')}</th></tr></thead>
            <tbody>
              {work_orders.map((wo: any) => (
                <tr key={wo.id}>
                  <td>#{wo.id}</td>
                  <td>{wo.description}</td>
                  <td>{wo.location || '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : <p>{t('bookings.noWorkOrders')}</p>}

      <h2 className="mt-2">{t('bookings.linkedQuotes')}</h2>
      {quotes.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead><tr><th>{t('common.id')}</th><th>{t('common.title')}</th><th>{t('common.status')}</th><th>{t('common.amount')}</th></tr></thead>
            <tbody>
              {quotes.map((q: any) => (
                <tr key={q.id}>
                  <td>#{q.id}</td>
                  <td>{q.title}</td>
                  <td><span className={`badge badge-${q.status}`}>{q.status}</span></td>
                  <td>{q.total_amount.toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : <p>{t('bookings.noLinkedQuotes')}</p>}
    </div>
  )
}
