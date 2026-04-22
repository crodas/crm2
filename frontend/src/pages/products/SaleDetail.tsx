import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function SaleDetail() {
  const { t } = useTranslation()
  const { id } = useParams()
  const qc = useQueryClient()
  const { data } = useQuery({
    queryKey: ['sale', id],
    queryFn: () => api.get<any>(`/sales/${id}`),
  })
  const { data: products } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })

  const [payAmount, setPayAmount] = useState(0)
  const [payMethod, setPayMethod] = useState('cash')
  const [payNotes, setPayNotes] = useState('')

  const payMutation = useMutation({
    mutationFn: () => api.post(`/sales/${id}/payments`, {
      amount: payAmount,
      method: payMethod || null,
      notes: payNotes || null,
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sale', id] })
      qc.invalidateQueries({ queryKey: ['receivables'] })
      qc.invalidateQueries({ queryKey: ['customers'] })
      setPayAmount(0)
      setPayNotes('')
    },
  })

  if (!data) return <p>{t('common.loading')}</p>

  const { sale, lines, payments, total_paid, balance } = data
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
          <p><strong>{t('sales.status_label')}</strong> {sale.payment_status === 'paid' ? t('sales.paid') : t('sales.credit')}</p>
          <p><strong>{t('sales.paid_label')}</strong> {total_paid.toLocaleString()}</p>
          <p><strong>{t('sales.balance_label')}</strong> {balance.toLocaleString()}</p>
          <p><strong>{t('sales.notes_label')}</strong> {sale.notes || '—'}</p>
        </div>

        {balance > 0 && (
          <div className="card">
            <h2>{t('sales.recordPayment')}</h2>
            <div className="form-group mb-1">
              <label>{t('common.amount')}</label>
              <input type="number" value={payAmount || ''} onChange={e => setPayAmount(Number(e.target.value))} />
            </div>
            <div className="form-group mb-1">
              <label>{t('common.method')}</label>
              <select value={payMethod} onChange={e => setPayMethod(e.target.value)}>
                <option value="cash">{t('sales.cash')}</option>
                <option value="card">{t('sales.card')}</option>
                <option value="transfer">{t('sales.transfer')}</option>
                <option value="check">{t('sales.check')}</option>
              </select>
            </div>
            <div className="form-group mb-1">
              <label>{t('common.notes')}</label>
              <input value={payNotes} onChange={e => setPayNotes(e.target.value)} />
            </div>
            <button
              className="btn btn-primary"
              onClick={() => payMutation.mutate()}
              disabled={!payAmount || payMutation.isPending}
            >
              {payMutation.isPending ? t('common.recording') : t('sales.recordPayment')}
            </button>
            {payMutation.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(payMutation.error as Error).message}</p>}
          </div>
        )}
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

      {payments.length > 0 && (
        <>
          <h2>{t('sales.paymentHistory')}</h2>
          <div className="table-wrap">
            <table>
              <thead>
                <tr><th>{t('common.date')}</th><th>{t('common.amount')}</th><th>{t('common.method')}</th><th>{t('common.notes')}</th></tr>
              </thead>
              <tbody>
                {payments.map((p: any) => (
                  <tr key={p.id}>
                    <td>{new Date(p.paid_at).toLocaleString()}</td>
                    <td>{p.amount.toLocaleString()}</td>
                    <td>{p.method || '—'}</td>
                    <td>{p.notes || '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  )
}
