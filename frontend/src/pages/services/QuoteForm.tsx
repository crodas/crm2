import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'

interface QLine {
  line_type: 'service' | 'product'
  service_id: number | null
  description: string
  quantity: number
  unit_price: number
}

export default function QuoteForm() {
  const nav = useNavigate()
  const qc = useQueryClient()
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })
  const { data: allProducts } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: customerGroups } = useQuery({ queryKey: ['customer-groups'], queryFn: () => api.get<any[]>('/customer-groups') })

  const products = allProducts?.filter((p: any) => p.product_type === 'product') ?? []
  const services = allProducts?.filter((p: any) => p.product_type === 'service') ?? []

  const [customerId, setCustomerId] = useState(0)
  const [title, setTitle] = useState('')
  const [description, setDescription] = useState('')
  const [lines, setLines] = useState<QLine[]>([])

  // Resolve customer group for price lookup
  const selectedCustomer = customers?.find((c: any) => c.id === customerId)
  const customerGroup = customerGroups?.find((g: any) => g.customer_type_id === selectedCustomer?.customer_type_id)

  // Fetch latest prices for the customer's group
  const { data: latestPrices } = useQuery({
    queryKey: ['latest-prices', customerGroup?.id],
    queryFn: () => api.get<any[]>(`/inventory/prices?customer_group_id=${customerGroup!.id}`),
    enabled: !!customerGroup,
  })

  const getProductPrice = (productId: number): number => {
    const p = latestPrices?.find((lp: any) => lp.product_id === productId)
    return p?.price_per_unit ?? 0
  }

  const addProduct = () => {
    const first = products[0]
    setLines([...lines, {
      line_type: 'product',
      service_id: first?.id ?? null,
      description: first?.name ?? '',
      quantity: 1,
      unit_price: first ? getProductPrice(first.id) : 0,
    }])
  }

  const addService = () => {
    const first = services[0]
    setLines([...lines, {
      line_type: 'service',
      service_id: first?.id ?? null,
      description: first?.name ?? '',
      quantity: 1,
      unit_price: first?.suggested_price ?? 0,
    }])
  }

  const updateLine = (idx: number, field: string, value: string | number | null) => {
    const updated = [...lines]
    ;(updated[idx] as any)[field] = value

    // When selecting a product/service, prefill description and price
    if (field === 'service_id' && value) {
      const item = allProducts?.find((p: any) => p.id === value)
      if (item) {
        updated[idx].description = item.name
        if (updated[idx].line_type === 'service') {
          updated[idx].unit_price = item.suggested_price
        } else {
          updated[idx].unit_price = getProductPrice(item.id)
        }
      }
    }
    setLines(updated)
  }

  // Re-fill product prices when latestPrices loads or customer changes
  useEffect(() => {
    if (!latestPrices || lines.length === 0) return
    setLines(prev => prev.map(line => {
      if (line.line_type === 'product' && line.service_id) {
        const price = getProductPrice(line.service_id)
        if (price > 0) return { ...line, unit_price: price }
      }
      return line
    }))
  }, [latestPrices])

  const removeLine = (idx: number) => setLines(lines.filter((_, i) => i !== idx))

  const total = lines.reduce((s, l) => s + l.quantity * l.unit_price, 0)

  const mutation = useMutation({
    mutationFn: () => api.post('/quotes', {
      customer_id: customerId,
      title,
      description: description || null,
      lines: lines.map(l => ({
        description: l.description,
        quantity: l.quantity,
        unit_price: l.unit_price,
        service_id: l.service_id,
        line_type: l.line_type,
      })),
    }),
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
              <strong>
                <span className={`badge ${line.line_type === 'service' ? 'badge-accepted' : 'badge-sent'}`}>
                  {line.line_type}
                </span>
                {' '}{idx + 1}
              </strong>
              <button className="btn btn-danger btn-sm" onClick={() => removeLine(idx)}>Remove</button>
            </div>

            <div className="form-group">
              <label>{line.line_type === 'service' ? 'Service' : 'Product'}</label>
              <select
                value={line.service_id ?? ''}
                onChange={e => updateLine(idx, 'service_id', Number(e.target.value))}
              >
                <option value="">Select...</option>
                {(line.line_type === 'service' ? services : products).map((p: any) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                    {line.line_type === 'service'
                      ? ` — ${p.suggested_price.toLocaleString()}`
                      : ''}
                  </option>
                ))}
              </select>
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

        <div className="flex gap-1 mt-1">
          <button className="btn" onClick={addProduct}>+ Add Product</button>
          <button className="btn" onClick={addService}>+ Add Service</button>
        </div>

        <div className="flex-between mt-2">
          <strong>Total: {total.toLocaleString()}</strong>
          <button
            className="btn btn-primary"
            onClick={() => mutation.mutate()}
            disabled={!customerId || !title || lines.length === 0 || mutation.isPending}
          >
            {mutation.isPending ? 'Saving...' : 'Create Quote'}
          </button>
        </div>
        {mutation.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(mutation.error as Error).message}</p>}
      </div>
    </div>
  )
}
