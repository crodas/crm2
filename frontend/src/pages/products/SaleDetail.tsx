import { useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { api } from '../../api'

export default function SaleDetail() {
  const { id } = useParams()
  const { data } = useQuery({
    queryKey: ['sale', id],
    queryFn: () => api.get<any>(`/sales/${id}`),
  })
  const { data: products } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })

  if (!data) return <p>Loading...</p>

  const { sale, lines } = data
  const customer = customers?.find((c: any) => c.id === sale.customer_id)
  const productName = (pid: number) => products?.find((p: any) => p.id === pid)?.name ?? `#${pid}`

  return (
    <div>
      <h1>Sale #{sale.id}</h1>
      <div className="grid-2 mb-2">
        <div className="card">
          <h2>Details</h2>
          <p><strong>Customer:</strong> {customer?.name ?? sale.customer_id}</p>
          <p><strong>Date:</strong> {new Date(sale.sold_at).toLocaleString()}</p>
          <p><strong>Total:</strong> {sale.total_amount.toLocaleString()}</p>
          <p><strong>Notes:</strong> {sale.notes || '—'}</p>
        </div>
      </div>

      <h2>Line Items</h2>
      <table>
        <thead>
          <tr><th>Product</th><th>Quantity</th><th>Price/Unit</th><th>Subtotal</th></tr>
        </thead>
        <tbody>
          {lines.map((l: any) => (
            <tr key={l.id}>
              <td>{productName(l.product_id)}</td>
              <td>{l.quantity}</td>
              <td>{l.price_per_unit.toLocaleString()}</td>
              <td>{(l.quantity * l.price_per_unit).toLocaleString()}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
