import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function InventoryView() {
  const { t } = useTranslation()
  const { data: stock } = useQuery({
    queryKey: ['stock'],
    queryFn: () => api.get<any[]>('/inventory/stock'),
  })
  const { data: products } = useQuery({
    queryKey: ['products'],
    queryFn: () => api.get<any[]>('/products'),
  })
  const { data: warehouses } = useQuery({
    queryKey: ['warehouses'],
    queryFn: () => api.get<any[]>('/warehouses'),
  })
  const { data: receipts } = useQuery({
    queryKey: ['receipts'],
    queryFn: () => api.get<any[]>('/inventory/receipts'),
  })

  const productName = (id: number) => products?.find((p: any) => p.id === id)?.name ?? `#${id}`
  const warehouseName = (id: number) => warehouses?.find((w: any) => w.id === id)?.name ?? `#${id}`

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>{t('inventory.currentStock')}</h1>
        <div className="flex gap-1">
          <Link to="/inventory/transfer" className="btn">{t('inventory.transfer')}</Link>
          <Link to="/inventory/receive" className="btn btn-primary">{t('inventory.receiveInventory')}</Link>
        </div>
      </div>

      <div className="table-wrap">
        <table>
          <thead>
            <tr><th>{t('sales.product')}</th><th>{t('sales.warehouse')}</th><th>{t('common.quantity')}</th></tr>
          </thead>
          <tbody>
            {stock?.map((s: any, i: number) => (
              <tr key={i}>
                <td>{productName(s.product_id)}</td>
                <td>{warehouseName(s.warehouse_id)}</td>
                <td><strong>{s.total_quantity}</strong></td>
              </tr>
            ))}
            {stock?.length === 0 && (
              <tr><td colSpan={3} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>{t('inventory.noStock')}</td></tr>
            )}
          </tbody>
        </table>
      </div>

      <h2 className="mt-2">{t('inventory.recentReceipts')}</h2>
      {receipts && receipts.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead>
              <tr><th>{t('inventory.reference')}</th><th>{t('inventory.supplier')}</th><th>{t('inventory.totalCost')}</th><th>{t('common.date')}</th></tr>
            </thead>
            <tbody>
              {receipts.map((r: any) => (
                <tr key={r.id}>
                  <td><Link to={`/inventory/receipts/${r.id}`}>{r.reference || `#${r.id}`}</Link></td>
                  <td>{r.supplier_name || '—'}</td>
                  <td>{r.total_cost.toLocaleString()}</td>
                  <td>{new Date(r.received_at).toLocaleDateString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : <p className="text-muted">{t('inventory.noReceipts')}</p>}
    </div>
  )
}
