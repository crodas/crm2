import { useState, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'

interface SaleLine {
  product_id: number
  warehouse_id: number
  quantity: number
  price_per_unit: number
}

export default function SaleForm() {
  const nav = useNavigate()
  const qc = useQueryClient()

  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })
  const { data: groups } = useQuery({ queryKey: ['customer-groups'], queryFn: () => api.get<any[]>('/customer-groups') })
  const { data: products } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: warehouses } = useQuery({ queryKey: ['warehouses'], queryFn: () => api.get<any[]>('/warehouses') })

  const [customerId, setCustomerId] = useState(0)
  const [notes, setNotes] = useState('')
  const [lines, setLines] = useState<SaleLine[]>([])

  // Auto-resolve customer group from customer type
  const selectedCustomer = useMemo(
    () => customers?.find((c: any) => c.id === customerId),
    [customers, customerId]
  )
  const customerGroup = useMemo(
    () => groups?.find((g: any) => g.customer_type_id === selectedCustomer?.customer_type_id),
    [groups, selectedCustomer]
  )

  const addLine = () => {
    setLines([...lines, {
      product_id: products?.[0]?.id ?? 0,
      warehouse_id: warehouses?.[0]?.id ?? 0,
      quantity: 1,
      price_per_unit: 0,
    }])
  }

  const updateLine = (idx: number, field: string, value: number) => {
    const updated = [...lines]
    ;(updated[idx] as any)[field] = value
    setLines(updated)
  }

  const removeLine = (idx: number) => setLines(lines.filter((_, i) => i !== idx))

  const total = lines.reduce((sum, l) => sum + l.quantity * l.price_per_unit, 0)

  const mutation = useMutation({
    mutationFn: () => api.post('/sales', {
      customer_id: customerId,
      customer_group_id: customerGroup?.id,
      notes: notes || null,
      lines,
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sales'] })
      qc.invalidateQueries({ queryKey: ['stock'] })
      nav('/sales')
    },
  })

  return (
    <div>
      <h1>New Sale</h1>
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
            <label>Price Group</label>
            <input value={customerGroup?.name ?? '—'} disabled style={{ background: 'var(--bg)' }} />
          </div>
        </div>
        <div className="form-group mb-2">
          <label>Notes</label>
          <input value={notes} onChange={e => setNotes(e.target.value)} />
        </div>

        <h2>Items</h2>
        {lines.map((line, idx) => (
          <div key={idx} className="card" style={{ background: 'var(--bg)' }}>
            <div className="flex-between mb-1">
              <strong>Item {idx + 1}</strong>
              <button className="btn btn-danger btn-sm" onClick={() => removeLine(idx)}>Remove</button>
            </div>
            <div className="grid-2">
              <div className="form-group">
                <label>Product</label>
                <select value={line.product_id} onChange={e => updateLine(idx, 'product_id', Number(e.target.value))}>
                  {products?.map((p: any) => <option key={p.id} value={p.id}>{p.name}</option>)}
                </select>
              </div>
              <div className="form-group">
                <label>Warehouse</label>
                <select value={line.warehouse_id} onChange={e => updateLine(idx, 'warehouse_id', Number(e.target.value))}>
                  {warehouses?.map((w: any) => <option key={w.id} value={w.id}>{w.name}</option>)}
                </select>
              </div>
              <div className="form-group">
                <label>Quantity</label>
                <input type="number" value={line.quantity} onChange={e => updateLine(idx, 'quantity', Number(e.target.value))} />
              </div>
              <div className="form-group">
                <label>Price per Unit</label>
                <input type="number" value={line.price_per_unit} onChange={e => updateLine(idx, 'price_per_unit', Number(e.target.value))} />
              </div>
            </div>
          </div>
        ))}

        <div className="flex-between mt-1">
          <button className="btn" onClick={addLine}>+ Add Item</button>
          <strong>Total: {total.toLocaleString()}</strong>
        </div>

        <button
          className="btn btn-primary mt-2"
          onClick={() => mutation.mutate()}
          disabled={!customerId || !customerGroup || lines.length === 0 || mutation.isPending}
        >
          {mutation.isPending ? 'Processing...' : 'Create Sale'}
        </button>
        {mutation.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(mutation.error as Error).message}</p>}
      </div>
    </div>
  )
}
