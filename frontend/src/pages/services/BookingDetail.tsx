import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'

export default function BookingDetail() {
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

  if (!data) return <p>Loading...</p>

  const { booking, work_orders, quotes } = data

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>Booking: {booking.title}</h1>
        <span className={`badge badge-${booking.status}`}>{booking.status}</span>
      </div>

      <div className="grid-2">
        <div className="card">
          <h2>Details</h2>
          <p><strong>Start:</strong> {new Date(booking.start_at).toLocaleString()}</p>
          <p><strong>End:</strong> {new Date(booking.end_at).toLocaleString()}</p>
          <p><strong>Notes:</strong> {booking.notes || '—'}</p>
        </div>
        <div className="card">
          <h2>Add Work Order</h2>
          <div className="form-group">
            <label>Description</label>
            <textarea value={woDesc} onChange={e => setWoDesc(e.target.value)} rows={2} />
          </div>
          <div className="form-group">
            <label>Location</label>
            <input value={woLocation} onChange={e => setWoLocation(e.target.value)} />
          </div>
          <button
            className="btn btn-primary"
            onClick={() => woMutation.mutate()}
            disabled={!woDesc || woMutation.isPending}
          >
            Add Work Order
          </button>
        </div>
      </div>

      <h2 className="mt-2">Work Orders</h2>
      {work_orders.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead><tr><th>ID</th><th>Description</th><th>Location</th></tr></thead>
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
      ) : <p>No work orders</p>}

      <h2 className="mt-2">Linked Quotes</h2>
      {quotes.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead><tr><th>ID</th><th>Title</th><th>Status</th><th>Amount</th></tr></thead>
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
      ) : <p>No linked quotes</p>}
    </div>
  )
}
