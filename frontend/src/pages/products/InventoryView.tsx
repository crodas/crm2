import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'

export default function InventoryView() {
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

  const productName = (id: number) => products?.find((p: any) => p.id === id)?.name ?? `#${id}`
  const warehouseName = (id: number) => warehouses?.find((w: any) => w.id === id)?.name ?? `#${id}`

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>Current Stock</h1>
        <Link to="/inventory/receive" className="btn btn-primary">Receive Inventory</Link>
      </div>

      <div className="table-wrap">
        <table>
          <thead>
            <tr><th>Product</th><th>Warehouse</th><th>Quantity</th></tr>
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
              <tr><td colSpan={3} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>No stock yet</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  )
}
