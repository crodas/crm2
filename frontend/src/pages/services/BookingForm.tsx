import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function BookingForm() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const qc = useQueryClient()
  const { data: teams } = useQuery({ queryKey: ['teams'], queryFn: () => api.get<any[]>('/teams') })
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })

  const [form, setForm] = useState({
    team_id: 0,
    customer_id: 0,
    title: '',
    start_at: '',
    end_at: '',
    notes: '',
  })

  const set = (field: string, value: string | number) => setForm(f => ({ ...f, [field]: value }))

  const mutation = useMutation({
    mutationFn: () => api.post('/bookings', { ...form, notes: form.notes || null }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['bookings'] }); nav('/calendar') },
  })

  return (
    <div>
      <h1>{t('bookings.newBooking')}</h1>
      <div className="card" style={{ maxWidth: 600 }}>
        <div className="grid-2 mb-1">
          <div className="form-group">
            <label>{t('bookings.team')}</label>
            <select value={form.team_id} onChange={e => set('team_id', Number(e.target.value))}>
              <option value={0}>{t('common.select')}</option>
              {teams?.map((t: any) => <option key={t.id} value={t.id}>{t.name}</option>)}
            </select>
          </div>
          <div className="form-group">
            <label>{t('bookings.customer')}</label>
            <select value={form.customer_id} onChange={e => set('customer_id', Number(e.target.value))}>
              <option value={0}>{t('common.select')}</option>
              {customers?.map((c: any) => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
          </div>
        </div>
        <div className="form-group">
          <label>{t('common.title')}</label>
          <input value={form.title} onChange={e => set('title', e.target.value)} />
        </div>
        <div className="grid-2">
          <div className="form-group">
            <label>{t('bookings.start')}</label>
            <input type="datetime-local" value={form.start_at} onChange={e => set('start_at', e.target.value)} />
          </div>
          <div className="form-group">
            <label>{t('bookings.end')}</label>
            <input type="datetime-local" value={form.end_at} onChange={e => set('end_at', e.target.value)} />
          </div>
        </div>
        <div className="form-group">
          <label>{t('common.notes')}</label>
          <textarea value={form.notes} onChange={e => set('notes', e.target.value)} rows={3} />
        </div>
        <button
          className="btn btn-primary"
          onClick={() => mutation.mutate()}
          disabled={!form.team_id || !form.customer_id || !form.title || !form.start_at || !form.end_at || mutation.isPending}
        >
          {mutation.isPending ? t('common.saving') : t('bookings.createBooking')}
        </button>
        {mutation.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(mutation.error as Error).message}</p>}
      </div>
    </div>
  )
}
