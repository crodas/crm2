import { useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function SaleDetail() {
  const { t } = useTranslation()
  const { id } = useParams()
  const { data } = useQuery({
    queryKey: ['sale', id],
    queryFn: () => api.get<any>(`/sales/${id}`),
  })
  const { data: products } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })

  if (!data) return <p>{t('common.loading')}</p>

  const { sale, lines } = data
  const customer = customers?.find((c: any) => c.id === sale.customer_id)
  const productName = (pid: number) => products?.find((p: any) => p.id === pid)?.name ?? `#${pid}`

  return (
    <div>
      <h1>{t('sales.saleNumber', { id: sale.id })}</h1>
      <div className="grid-2 mb-2">
        <div className="card">
          <h2>{t('common.details')}</h2>
          <p><strong>{t('sales.customer_label')}</strong> {customer?.name ?? sale.customer_id}</p>
          <p><strong>{t('sales.date_label')}</strong> {new Date(sale.sold_at).toLocaleString()}</p>
          <p><strong>{t('sales.total_label')}</strong> {sale.total_amount.toLocaleString()}</p>
          <p><strong>{t('sales.notes_label')}</strong> {sale.notes || '—'}</p>
        </div>
      </div>

      <h2>{t('sales.lineItems')}</h2>
      <div className="table-wrap">
        <table>
          <thead>
            <tr><th>{t('sales.product')}</th><th>{t('common.quantity')}</th><th>{t('sales.priceUnit')}</th><th>{t('sales.subtotal')}</th></tr>
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
    </div>
  )
}
