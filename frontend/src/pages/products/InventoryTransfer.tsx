import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

interface TransferLine {
  product_id: number
  quantity: number
}

export default function InventoryTransfer() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const qc = useQueryClient()

  const { data: products } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: warehouses } = useQuery({ queryKey: ['warehouses'], queryFn: () => api.get<any[]>('/warehouses') })
  const { data: stock } = useQuery({ queryKey: ['stock'], queryFn: () => api.get<any[]>('/inventory/stock') })

  const [fromWarehouseId, setFromWarehouseId] = useState<number>(0)
  const [toWarehouseId, setToWarehouseId] = useState<number>(0)
  const [lines, setLines] = useState<TransferLine[]>([])

  // Set defaults once warehouses load
  if (warehouses && warehouses.length >= 2) {
    if (fromWarehouseId === 0) setFromWarehouseId(warehouses[0].id)
    if (toWarehouseId === 0) setToWarehouseId(warehouses[1].id)
  } else if (warehouses && warehouses.length === 1 && fromWarehouseId === 0) {
    setFromWarehouseId(warehouses[0].id)
  }

  const addLine = () => {
    setLines([...lines, {
      product_id: products?.[0]?.id ?? 0,
      quantity: 1,
    }])
  }

  const updateLine = (idx: number, field: keyof TransferLine, value: number) => {
    const updated = [...lines]
    updated[idx] = { ...updated[idx], [field]: value }
    setLines(updated)
  }

  const removeLine = (idx: number) => setLines(lines.filter((_, i) => i !== idx))

  const availableQty = (productId: number) => {
    const s = stock?.find((s: any) => s.product_id === productId && s.warehouse_id === fromWarehouseId)
    return s?.total_quantity ?? 0
  }

  const productName = (id: number) => products?.find((p: any) => p.id === id)?.name ?? `#${id}`

  const sameWarehouse = fromWarehouseId === toWarehouseId && fromWarehouseId !== 0

  const mutation = useMutation({
    mutationFn: () => api.post('/inventory/transfer', {
      from_warehouse_id: fromWarehouseId,
      to_warehouse_id: toWarehouseId,
      lines,
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['stock'] })
      nav('/inventory')
    },
  })

  return (
    <div>
      <h1>{t('inventory.transferInventory')}</h1>
      <div className="card">
        <div className="grid-2 mb-2">
          <div className="form-group">
            <label>{t('inventory.fromWarehouse')}</label>
            <select value={fromWarehouseId} onChange={e => setFromWarehouseId(Number(e.target.value))}>
              {warehouses?.map((w: any) => <option key={w.id} value={w.id}>{w.name}</option>)}
            </select>
          </div>
          <div className="form-group">
            <label>{t('inventory.toWarehouse')}</label>
            <select value={toWarehouseId} onChange={e => setToWarehouseId(Number(e.target.value))}>
              {warehouses?.map((w: any) => <option key={w.id} value={w.id}>{w.name}</option>)}
            </select>
          </div>
        </div>

        {sameWarehouse && (
          <p style={{ color: 'red', marginBottom: '0.5rem' }}>{t('inventory.sameWarehouseError')}</p>
        )}

        <h2>{t('inventory.lineItems')}</h2>
        {lines.map((line, idx) => {
          const avail = availableQty(line.product_id)
          return (
            <div key={idx} className="card" style={{ background: 'var(--bg-app)' }}>
              <div className="flex-between mb-1">
                <strong>{t('inventory.item', { n: idx + 1 })}</strong>
                <button className="btn btn-danger btn-sm" onClick={() => removeLine(idx)}>{t('common.remove')}</button>
              </div>
              <div className="form-group mb-1">
                <label>{t('sales.product')}</label>
                <select value={line.product_id} onChange={e => updateLine(idx, 'product_id', Number(e.target.value))}>
                  {products?.map((p: any) => <option key={p.id} value={p.id}>{p.name}</option>)}
                </select>
              </div>
              <div className="grid-2 mb-1">
                <div className="form-group">
                  <label>{t('common.quantity')}</label>
                  <input type="number" min={0} step="any" value={line.quantity} onChange={e => updateLine(idx, 'quantity', Number(e.target.value))} />
                </div>
                <div className="form-group">
                  <label style={{ color: 'var(--text-muted)', fontSize: '0.85rem' }}>
                    {t('inventory.currentStock')}: {avail} {productName(line.product_id)}
                  </label>
                </div>
              </div>
              {line.quantity > avail && (
                <p style={{ color: 'red', fontSize: '0.85rem' }}>
                  {t('common.quantity')} &gt; {t('inventory.currentStock').toLowerCase()} ({avail})
                </p>
              )}
            </div>
          )
        })}

        <div className="flex gap-1 mt-1">
          <button className="btn" onClick={addLine}>{t('inventory.addLine')}</button>
          <button
            className="btn btn-primary"
            onClick={() => mutation.mutate()}
            disabled={lines.length === 0 || sameWarehouse || mutation.isPending}
          >
            {mutation.isPending ? t('common.processing') : t('inventory.transfer')}
          </button>
        </div>
        {mutation.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(mutation.error as Error).message}</p>}
      </div>
    </div>
  )
}
