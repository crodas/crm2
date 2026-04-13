import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function CustomerForm() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const qc = useQueryClient()
  const { data: types } = useQuery({
    queryKey: ['customer-types'],
    queryFn: () => api.get<any[]>('/customer-types'),
  })

  const [form, setForm] = useState({
    customer_type_id: 1,
    name: '',
    email: '',
    phone: '',
    address: '',
    notes: '',
  })

  const mutation = useMutation({
    mutationFn: () => api.post('/customers', form),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['customers'] }); nav('/customers') },
  })

  const set = (field: string, value: string | number) => setForm(f => ({ ...f, [field]: value }))

  return (
    <div>
      <h1>{t('customers.newCustomer')}</h1>
      <div className="card" style={{ maxWidth: 500 }}>
        <div className="form-group">
          <label>{t('common.type')}</label>
          <select value={form.customer_type_id} onChange={e => set('customer_type_id', Number(e.target.value))}>
            {types?.map((t: any) => <option key={t.id} value={t.id}>{t.name}</option>)}
          </select>
        </div>
        <div className="form-group">
          <label>{t('customers.nameRequired')}</label>
          <input value={form.name} onChange={e => set('name', e.target.value)} />
        </div>
        <div className="grid-2">
          <div className="form-group">
            <label>{t('common.phone')}</label>
            <input value={form.phone} onChange={e => set('phone', e.target.value)} />
          </div>
          <div className="form-group">
            <label>{t('common.email')}</label>
            <input value={form.email} onChange={e => set('email', e.target.value)} />
          </div>
        </div>
        <div className="form-group">
          <label>{t('common.address')}</label>
          <input value={form.address} onChange={e => set('address', e.target.value)} />
        </div>
        <div className="form-group">
          <label>{t('common.notes')}</label>
          <textarea value={form.notes} onChange={e => set('notes', e.target.value)} rows={3} />
        </div>
        <button className="btn btn-primary" onClick={() => mutation.mutate()} disabled={!form.name}>
          {mutation.isPending ? t('common.saving') : t('customers.createCustomer')}
        </button>
        {mutation.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(mutation.error as Error).message}</p>}
      </div>
    </div>
  )
}
