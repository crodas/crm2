import { useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function ProductForm() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const qc = useQueryClient()
  const [searchParams] = useSearchParams()
  const defaultType = searchParams.get('type') || 'product'

  const [form, setForm] = useState({
    sku: '',
    name: '',
    description: '',
    unit: 'unit',
    product_type: defaultType,
    suggested_price: 0,
  })

  const mutation = useMutation({
    mutationFn: () => api.post('/products', form),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['products'] }); nav('/products') },
  })

  const set = (field: string, value: string | number) => setForm(f => ({ ...f, [field]: value }))
  const isService = form.product_type === 'service'

  return (
    <div>
      <h1>{isService ? t('products.newService') : t('products.newProduct')}</h1>
      <div className="card" style={{ maxWidth: 500 }}>
        <div className="form-group">
          <label>{t('common.type')}</label>
          <select value={form.product_type} onChange={e => set('product_type', e.target.value)}>
            <option value="product">{t('products.product')}</option>
            <option value="service">{t('products.service')}</option>
          </select>
        </div>
        {!isService && (
          <div className="grid-2">
            <div className="form-group">
              <label>{t('products.sku')}</label>
              <input value={form.sku} onChange={e => set('sku', e.target.value)} />
            </div>
            <div className="form-group">
              <label>{t('products.unit')}</label>
              <input value={form.unit} onChange={e => set('unit', e.target.value)} />
            </div>
          </div>
        )}
        <div className="form-group">
          <label>{t('customers.nameRequired')}</label>
          <input value={form.name} onChange={e => set('name', e.target.value)} />
        </div>
        <div className="form-group">
          <label>{t('common.description')}</label>
          <textarea value={form.description} onChange={e => set('description', e.target.value)} rows={3} />
        </div>
        {isService && (
          <div className="form-group">
            <label>{t('products.suggestedPrice')}</label>
            <input type="number" value={form.suggested_price} onChange={e => set('suggested_price', Number(e.target.value))} />
          </div>
        )}
        <button className="btn btn-primary" onClick={() => mutation.mutate()} disabled={!form.name}>
          {mutation.isPending ? t('common.saving') : (isService ? t('products.createService') : t('products.createProduct'))}
        </button>
      </div>
    </div>
  )
}
