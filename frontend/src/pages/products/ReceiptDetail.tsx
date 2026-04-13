import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function ReceiptDetail() {
  const { t } = useTranslation()
  const { id } = useParams()
  const qc = useQueryClient()

  const { data } = useQuery({
    queryKey: ['receipt', id],
    queryFn: () => api.get<any>(`/inventory/receipts/${id}`),
  })
  const { data: products } = useQuery({ queryKey: ['products'], queryFn: () => api.get<any[]>('/products') })
  const { data: warehouses } = useQuery({ queryKey: ['warehouses'], queryFn: () => api.get<any[]>('/warehouses') })

  const [payAmount, setPayAmount] = useState(0)
  const [payMethod, setPayMethod] = useState('cash')
  const [payNotes, setPayNotes] = useState('')

  const payMutation = useMutation({
    mutationFn: () => api.post(`/inventory/receipts/${id}/payments`, {
      amount: payAmount,
      method: payMethod,
      notes: payNotes || null,
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['receipt', id] })
      qc.invalidateQueries({ queryKey: ['supplier-balance'] })
      setPayAmount(0)
      setPayNotes('')
    },
  })

  if (!data) return <p>{t('common.loading')}</p>

  const { receipt, utxos, ledger, total_paid, balance } = data
  const productName = (pid: number) => products?.find((p: any) => p.id === pid)?.name ?? `#${pid}`
  const warehouseName = (wid: number) => warehouses?.find((w: any) => w.id === wid)?.name ?? `#${wid}`
  const hasDebt = balance < 0

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>
          {t('inventory.receiptNumber', { id: receipt.id })}
          {hasDebt && <span className="badge badge-follow_up" style={{ marginLeft: '0.5rem' }}>{t('inventory.debt')}</span>}
        </h1>
      </div>

      <div className="grid-2">
        <div className="card">
          <h2>{t('common.details')}</h2>
          <p><strong>{t('inventory.reference_label')}</strong> {receipt.reference || '—'}</p>
          <p><strong>{t('inventory.supplier_label')}</strong> {receipt.supplier_name || '—'}</p>
          <p><strong>{t('inventory.totalCost')}</strong> {receipt.total_cost.toLocaleString()}</p>
          <p><strong>{t('inventory.paid_label')}</strong> {total_paid.toLocaleString()}</p>
          <p><strong>{t('inventory.balance_label')}</strong> <span style={{ color: balance < 0 ? 'var(--status-danger)' : 'var(--status-success)' }}>
            {balance.toLocaleString()}
          </span></p>
          <p><strong>{t('inventory.received_label')}</strong> {new Date(receipt.received_at).toLocaleDateString()}</p>
          <p><strong>{t('common.versionId')}</strong> <code title={receipt.version_id}>{receipt.version_id?.slice(0, 8)}</code></p>
        </div>

        {hasDebt && (
          <div className="card">
            <h2>{t('inventory.recordPayment')}</h2>
            <div className="form-group">
              <label>{t('common.amount')}</label>
              <input type="number" value={payAmount} onChange={e => setPayAmount(Number(e.target.value))} />
            </div>
            <div className="form-group">
              <label>{t('common.method')}</label>
              <select value={payMethod} onChange={e => setPayMethod(e.target.value)}>
                <option value="cash">{t('quotes.cash')}</option>
                <option value="card">{t('quotes.card')}</option>
                <option value="transfer">{t('quotes.transfer')}</option>
                <option value="check">{t('quotes.check')}</option>
              </select>
            </div>
            <div className="form-group">
              <label>{t('common.notes')}</label>
              <input value={payNotes} onChange={e => setPayNotes(e.target.value)} />
            </div>
            <button
              className="btn btn-primary"
              onClick={() => payMutation.mutate()}
              disabled={payAmount <= 0 || payMutation.isPending}
            >
              {payMutation.isPending ? t('common.recording') : t('inventory.recordPayment')}
            </button>
          </div>
        )}
      </div>

      <h2 className="mt-2">{t('inventory.lineItems')}</h2>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t('sales.product')}</th><th>{t('sales.warehouse')}</th><th>{t('common.quantity')}</th><th>{t('inventory.costPerUnit')}</th><th>{t('common.total')}</th></tr></thead>
          <tbody>
            {utxos.map((u: any) => (
              <tr key={u.id}>
                <td>{productName(u.product_id)}</td>
                <td>{warehouseName(u.warehouse_id)}</td>
                <td>{u.quantity}</td>
                <td>{u.cost_per_unit.toLocaleString()}</td>
                <td>{(u.quantity * u.cost_per_unit).toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <h2 className="mt-2">{t('inventory.paymentHistory')}</h2>
      {ledger && ledger.length > 0 ? (
        <div className="table-wrap">
          <table>
            <thead><tr><th>{t('common.versionId')}</th><th>{t('common.date')}</th><th>{t('common.amount')}</th><th>{t('common.method')}</th><th>{t('common.notes')}</th></tr></thead>
            <tbody>
              {ledger.map((e: any) => (
                <tr key={e.id}>
                  <td><code title={e.version_id}>{e.version_id?.slice(0, 8)}</code></td>
                  <td>{new Date(e.created_at).toLocaleDateString()}</td>
                  <td style={{ color: e.amount < 0 ? 'var(--status-danger)' : 'var(--status-success)' }}>
                    {e.amount.toLocaleString()}
                  </td>
                  <td>{e.method || '—'}</td>
                  <td>{e.notes || '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : <p>{t('inventory.noPayments')}</p>}
    </div>
  )
}
