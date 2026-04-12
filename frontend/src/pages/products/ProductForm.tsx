import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'

export default function ProductForm() {
  const nav = useNavigate()
  const qc = useQueryClient()
  const [form, setForm] = useState({ sku: '', name: '', description: '', unit: 'unit' })

  const mutation = useMutation({
    mutationFn: () => api.post('/products', form),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['products'] }); nav('/products') },
  })

  const set = (field: string, value: string) => setForm(f => ({ ...f, [field]: value }))

  return (
    <div>
      <h1>New Product</h1>
      <div className="card" style={{ maxWidth: 500 }}>
        <div className="grid-2">
          <div className="form-group">
            <label>SKU</label>
            <input value={form.sku} onChange={e => set('sku', e.target.value)} />
          </div>
          <div className="form-group">
            <label>Unit</label>
            <input value={form.unit} onChange={e => set('unit', e.target.value)} />
          </div>
        </div>
        <div className="form-group">
          <label>Name *</label>
          <input value={form.name} onChange={e => set('name', e.target.value)} />
        </div>
        <div className="form-group">
          <label>Description</label>
          <textarea value={form.description} onChange={e => set('description', e.target.value)} rows={3} />
        </div>
        <button className="btn btn-primary" onClick={() => mutation.mutate()} disabled={!form.name}>
          {mutation.isPending ? 'Saving...' : 'Create Product'}
        </button>
      </div>
    </div>
  )
}
