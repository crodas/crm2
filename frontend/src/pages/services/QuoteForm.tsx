import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'

interface QLine { description: string; quantity: number; unit_price: number }

export default function QuoteForm() {
  const nav = useNavigate()
  const qc = useQueryClient()
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })

  const [customerId, setCustomerId] = useState(0)
  const [title, setTitle] = useState('')
  const [description, setDescription] = useState('')
  const [lines, setLines] = useState<QLine[]>([])

  const addLine = () => setLines([...lines, { description: '', quantity: 1, unit_price: 0 }])
  const updateLine = (idx: number, field: string, value: string | number) => {
    const updated = [...lines]
    ;(updated[idx] as any)[field] = value
    setLines(updated)
  }
  const removeLine = (idx: number) => setLines(lines.filter((_, i) => i !== idx))

  const total = lines.reduce((s, l) => s + l.quantity * l.unit_price, 0)

  const mutation = useMutation({
    mutationFn: () => api.post('/quotes', { customer_id: customerId, title, description: description || null, lines }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['quotes'] }); nav('/quotes') },
  })

  return (
    <div>
      <h1>New Quote</h1>
      <div className="card">
        <div className="grid-2 mb-2">
          <div className="form-group">
            <label>Customer</label>
            <select value={customerId} onChange={e => setCustomerId(Number(e.target.value))}>
              <option value={0}>Select...</option>
              {customers?.map((c: any) => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
          </div>
          <div className="form-group">
            <label>Title</label>
            <input value={title} onChange={e => setTitle(e.target.value)} />
          </div>
        </div>
        <div className="form-group mb-2">
          <label>Description</label>
          <textarea value={description} onChange={e => setDescription(e.target.value)} rows={2} />
        </div>

        <h2>Line Items</h2>
        {lines.map((line, idx) => (
          <div key={idx} className="card" style={{ background: 'var(--bg)' }}>
            <div className="flex-between mb-1">
              <strong>Item {idx + 1}</strong>
              <button className="btn btn-danger btn-sm" onClick={() => removeLine(idx)}>Remove</button>
            </div>
            <div className="form-group">
              <label>Description</label>
              <input value={line.description} onChange={e => updateLine(idx, 'description', e.target.value)} />
            </div>
            <div className="grid-2">
              <div className="form-group">
                <label>Quantity</label>
                <input type="number" value={line.quantity} onChange={e => updateLine(idx, 'quantity', Number(e.target.value))} />
              </div>
              <div className="form-group">
                <label>Unit Price</label>
                <input type="number" value={line.unit_price} onChange={e => updateLine(idx, 'unit_price', Number(e.target.value))} />
              </div>
            </div>
          </div>
        ))}

        <div className="flex-between mt-1">
          <button className="btn" onClick={addLine}>+ Add Line</button>
          <strong>Total: {total.toLocaleString()}</strong>
        </div>

        <button
          className="btn btn-primary mt-2"
          onClick={() => mutation.mutate()}
          disabled={!customerId || !title || lines.length === 0 || mutation.isPending}
        >
          {mutation.isPending ? 'Saving...' : 'Create Quote'}
        </button>
        {mutation.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(mutation.error as Error).message}</p>}
      </div>
    </div>
  )
}
