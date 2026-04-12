import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'

export default function QuoteDetail() {
  const { id } = useParams()
  const qc = useQueryClient()

  const { data } = useQuery({
    queryKey: ['quote', id],
    queryFn: () => api.get<any>(`/quotes/${id}`),
  })

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

  if (!data) return <p>Loading...</p>

  const { quote, lines, payments, total_paid, balance } = data
  const statuses = ['draft', 'sent', 'follow_up', 'accepted', 'booked']

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>
          Quote #{quote.id}: {quote.title}
          {quote.is_debt ? <span className="badge badge-follow_up" style={{ marginLeft: '0.5rem' }}>debt</span> : null}
        </h1>
        <span className={`badge badge-${quote.status}`}>{quote.status}</span>
      </div>

      <div className="grid-2">
        <div className="card">
          <h2>Details</h2>
          <p><strong>Description:</strong> {quote.description || '—'}</p>
          <p><strong>Total:</strong> {quote.total_amount.toLocaleString()}</p>
          <p><strong>Paid:</strong> {total_paid.toLocaleString()}</p>
          <p><strong>Balance:</strong> <span style={{ color: balance > 0 ? 'var(--danger)' : 'var(--success)' }}>
            {balance.toLocaleString()}
          </span></p>
          <p><strong>Created:</strong> {new Date(quote.created_at).toLocaleDateString()}</p>

          <h2 className="mt-2">Status</h2>
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
          <h2>Record Payment</h2>
          <div className="form-group">
            <label>Amount</label>
            <input type="number" value={payAmount} onChange={e => setPayAmount(Number(e.target.value))} />
          </div>
          <div className="form-group">
            <label>Method</label>
            <select value={payMethod} onChange={e => setPayMethod(e.target.value)}>
              <option value="cash">Cash</option>
              <option value="card">Card</option>
              <option value="transfer">Transfer</option>
              <option value="check">Check</option>
            </select>
          </div>
          <div className="form-group">
            <label>Notes</label>
            <input value={payNotes} onChange={e => setPayNotes(e.target.value)} />
          </div>
          <button
            className="btn btn-primary"
            onClick={() => payMutation.mutate()}
            disabled={payAmount <= 0 || payMutation.isPending}
          >
            {payMutation.isPending ? 'Recording...' : 'Record Payment'}
          </button>
        </div>
      </div>

      <h2 className="mt-2">Line Items</h2>
      <table>
        <thead><tr><th>Description</th><th>Qty</th><th>Unit Price</th><th>Total</th></tr></thead>
        <tbody>
          {lines.map((l: any) => (
            <tr key={l.id}>
              <td>{l.description}</td>
              <td>{l.quantity}</td>
              <td>{l.unit_price.toLocaleString()}</td>
              <td>{(l.quantity * l.unit_price).toLocaleString()}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2 className="mt-2">Payment History</h2>
      {payments.length > 0 ? (
        <table>
          <thead><tr><th>Date</th><th>Amount</th><th>Method</th><th>Notes</th></tr></thead>
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
      ) : <p>No payments recorded</p>}
    </div>
  )
}
