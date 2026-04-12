import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'

export default function DebtForm() {
  const nav = useNavigate()
  const qc = useQueryClient()
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })

  const [customerId, setCustomerId] = useState(0)
  const [title, setTitle] = useState('')
  const [description, setDescription] = useState('')
  const [amount, setAmount] = useState(0)

  const mutation = useMutation({
    mutationFn: () => api.post('/debts', {
      customer_id: customerId,
      title,
      description: description || null,
      amount,
    }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['quotes'] }); nav('/quotes') },
  })

  return (
    <div>
      <h1>Quick Debt</h1>
      <div className="card" style={{ maxWidth: 500 }}>
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
        <div className="form-group">
          <label>Description</label>
          <textarea value={description} onChange={e => setDescription(e.target.value)} rows={2} />
        </div>
        <div className="form-group">
          <label>Amount</label>
          <input type="number" value={amount} onChange={e => setAmount(Number(e.target.value))} />
        </div>
        <button
          className="btn btn-primary"
          onClick={() => mutation.mutate()}
          disabled={!customerId || !title || amount <= 0 || mutation.isPending}
        >
          {mutation.isPending ? 'Saving...' : 'Create Debt'}
        </button>
        {mutation.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(mutation.error as Error).message}</p>}
      </div>
    </div>
  )
}
