import { useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../api'

export default function Settings() {
  const qc = useQueryClient()
  const { data: config } = useQuery({ queryKey: ['config'], queryFn: () => api.get<any>('/config') })

  const [form, setForm] = useState<Record<string, string>>({})

  useEffect(() => {
    if (config) {
      const flat: Record<string, string> = {}
      for (const [k, v] of Object.entries(config)) {
        flat[k] = typeof v === 'string' ? v : JSON.stringify(v)
      }
      setForm(flat)
    }
  }, [config])

  const mutation = useMutation({
    mutationFn: () => api.put('/config', form),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['config'] }),
  })

  const set = (key: string, value: string) => setForm(f => ({ ...f, [key]: value }))

  const fields = [
    { key: 'company_name', label: 'Company Name' },
    { key: 'company_address', label: 'Company Address' },
    { key: 'company_phone', label: 'Company Phone' },
    { key: 'company_tax_id', label: 'Tax ID' },
    { key: 'currency', label: 'Currency Code' },
    { key: 'currency_symbol', label: 'Currency Symbol' },
    { key: 'currency_decimals', label: 'Currency Decimals' },
    { key: 'quote_validity_days', label: 'Quote Validity (days)' },
    { key: 'quote_followup_days', label: 'Quote Follow-up (days)' },
    { key: 'inventory_costing_method', label: 'Costing Method' },
    { key: 'default_payment_methods', label: 'Payment Methods (JSON)' },
    { key: 'units', label: 'Available Units (JSON)' },
  ]

  return (
    <div>
      <h1>Settings</h1>
      <div className="card" style={{ maxWidth: 600 }}>
        {fields.map(f => (
          <div key={f.key} className="form-group">
            <label>{f.label}</label>
            <input value={form[f.key] ?? ''} onChange={e => set(f.key, e.target.value)} />
          </div>
        ))}
        <button
          className="btn btn-primary mt-1"
          onClick={() => mutation.mutate()}
          disabled={mutation.isPending}
        >
          {mutation.isPending ? 'Saving...' : 'Save Settings'}
        </button>
        {mutation.isSuccess && <p style={{ color: 'var(--success)', marginTop: '0.5rem' }}>Settings saved</p>}
      </div>
    </div>
  )
}
