import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

interface LinePrice {
  customer_group_id: number
  price_per_unit: number
}

interface Line {
  product_id: number
  warehouse_id: number
  quantity: number
  cost_per_unit: number
  prices: LinePrice[]
}

export default function InventoryReceive() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const qc = useQueryClient()

  const { data: products } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: warehouses } = useQuery({ queryKey: ['warehouses'], queryFn: () => api.get<any[]>('/warehouses') })
  const { data: groups } = useQuery({ queryKey: ['customer-groups'], queryFn: () => api.get<any[]>('/customer-groups') })

  const [reference, setReference] = useState('')
  const [supplier, setSupplier] = useState('')
  const [lines, setLines] = useState<Line[]>([])

  const addLine = () => {
    const prices = (groups || []).map((g: any) => ({
      customer_group_id: g.id,
      price_per_unit: 0,
    }))
    setLines([...lines, {
      product_id: products?.[0]?.id ?? 0,
      warehouse_id: warehouses?.[0]?.id ?? 0,
      quantity: 1,
      cost_per_unit: 0,
      prices,
    }])
  }

  const updateLine = (idx: number, field: string, value: number) => {
    const updated = [...lines]
    ;(updated[idx] as any)[field] = value

    // Auto-calculate prices from markup when cost changes
    if (field === 'cost_per_unit' && groups) {
      updated[idx].prices = groups.map((g: any) => ({
        customer_group_id: g.id,
        price_per_unit: Math.round(value * (1 + g.default_markup_pct / 100)),
      }))
    }
    setLines(updated)
  }

  const updatePrice = (lineIdx: number, groupId: number, price: number) => {
    const updated = [...lines]
    const p = updated[lineIdx].prices.find(p => p.customer_group_id === groupId)
    if (p) p.price_per_unit = price
    setLines(updated)
  }

  const removeLine = (idx: number) => setLines(lines.filter((_, i) => i !== idx))

  const mutation = useMutation({
    mutationFn: () => api.post('/inventory/receive', {
      reference: reference || null,
      supplier_name: supplier || null,
      lines,
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['stock'] })
      nav('/inventory')
    },
  })

  return (
    <div>
      <h1>{t('inventory.receiveInventory')}</h1>
      <div className="card">
        <div className="grid-2 mb-2">
          <div className="form-group">
            <label>{t('inventory.reference')}</label>
            <input value={reference} onChange={e => setReference(e.target.value)} />
          </div>
          <div className="form-group">
            <label>{t('inventory.supplier')}</label>
            <input value={supplier} onChange={e => setSupplier(e.target.value)} />
          </div>
        </div>

        <h2>{t('inventory.lineItems')}</h2>
        {lines.map((line, idx) => (
          <div key={idx} className="card" style={{ background: 'var(--bg-app)' }}>
            <div className="flex-between mb-1">
              <strong>{t('inventory.item', { n: idx + 1 })}</strong>
              <button className="btn btn-danger btn-sm" onClick={() => removeLine(idx)}>{t('common.remove')}</button>
            </div>
            <div className="grid-2 mb-1">
              <div className="form-group">
                <label>{t('sales.product')}</label>
                <select value={line.product_id} onChange={e => updateLine(idx, 'product_id', Number(e.target.value))}>
                  {products?.map((p: any) => <option key={p.id} value={p.id}>{p.name}</option>)}
                </select>
              </div>
              <div className="form-group">
                <label>{t('sales.warehouse')}</label>
                <select value={line.warehouse_id} onChange={e => updateLine(idx, 'warehouse_id', Number(e.target.value))}>
                  {warehouses?.map((w: any) => <option key={w.id} value={w.id}>{w.name}</option>)}
                </select>
              </div>
            </div>
            <div className="grid-2 mb-1">
              <div className="form-group">
                <label>{t('common.quantity')}</label>
                <input type="number" value={line.quantity} onChange={e => updateLine(idx, 'quantity', Number(e.target.value))} />
              </div>
              <div className="form-group">
                <label>{t('inventory.costPerUnit')}</label>
                <input type="number" value={line.cost_per_unit} onChange={e => updateLine(idx, 'cost_per_unit', Number(e.target.value))} />
              </div>
            </div>
            <div className="mb-1">
              <label style={{ fontSize: '0.85rem', fontWeight: 500 }}>{t('inventory.pricesByGroup')}</label>
              <div className="grid-2">
                {groups?.map((g: any) => {
                  const p = line.prices.find(p => p.customer_group_id === g.id)
                  return (
                    <div key={g.id} className="form-group">
                      <label>{g.name} ({t('inventory.markup', { pct: g.default_markup_pct })})</label>
                      <input
                        type="number"
                        value={p?.price_per_unit ?? 0}
                        onChange={e => updatePrice(idx, g.id, Number(e.target.value))}
                      />
                    </div>
                  )
                })}
              </div>
            </div>
          </div>
        ))}

        <div className="flex gap-1 mt-1">
          <button className="btn" onClick={addLine}>{t('inventory.addLine')}</button>
          <button
            className="btn btn-primary"
            onClick={() => mutation.mutate()}
            disabled={lines.length === 0 || mutation.isPending}
          >
            {mutation.isPending ? t('common.saving') : t('inventory.receiveInventory')}
          </button>
        </div>
        {mutation.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(mutation.error as Error).message}</p>}
      </div>
    </div>
  )
}
