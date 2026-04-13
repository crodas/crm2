import { useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function BookingDetail() {
  const { t } = useTranslation()
  const { id } = useParams()

  const { data } = useQuery({
    queryKey: ['booking', id],
    queryFn: () => api.get<any>(`/bookings/${id}`),
  })

  if (!data) return <p>{t('common.loading')}</p>

  const { booking, quotes } = data

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
          <p><strong>{t('common.description')}</strong> {booking.description || '—'}</p>
          <p><strong>{t('common.location')}</strong> {booking.location || '—'}</p>
          <p><strong>{t('bookings.notes_label')}</strong> {booking.notes || '—'}</p>
          <p><strong>{t('common.versionId')}</strong> <code title={booking.version_id}>{booking.version_id?.slice(0, 8)}</code></p>
        </div>
      </div>

      <h2 className="mt-2">{t('bookings.linkedQuotes')}</h2>
      {quotes.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead><tr><th>{t('common.id')}</th><th>{t('common.versionId')}</th><th>{t('common.title')}</th><th>{t('common.status')}</th><th>{t('common.amount')}</th></tr></thead>
            <tbody>
              {quotes.map((q: any) => (
                <tr key={q.id}>
                  <td>#{q.id}</td>
                  <td><code title={q.version_id}>{q.version_id?.slice(0, 8)}</code></td>
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
