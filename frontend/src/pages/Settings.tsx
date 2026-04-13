import { useState, useEffect, useRef } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../api'
import DragList from '../components/DragList'

// Inline editable cell: double-click to edit, Enter/blur to save
function EditableCell({ value, onSave }: { value: string; onSave: (v: string) => void }) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(value)
  const ref = useRef<HTMLInputElement>(null)

  useEffect(() => { setDraft(value) }, [value])
  useEffect(() => { if (editing) ref.current?.focus() }, [editing])

  if (!editing) {
    return (
      <span onDoubleClick={() => setEditing(true)} style={{ cursor: 'pointer' }}>
        {value || '—'}
      </span>
    )
  }

  const commit = () => {
    setEditing(false)
    if (draft !== value) onSave(draft)
  }

  return (
    <input
      ref={ref}
      value={draft}
      onChange={e => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={e => { if (e.key === 'Enter') commit(); if (e.key === 'Escape') { setDraft(value); setEditing(false) } }}
      style={{ width: '100%', padding: '0.2rem 0.4rem', fontSize: '0.9rem' }}
    />
  )
}

function EditableNumber({ value, onSave }: { value: number; onSave: (v: number) => void }) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(String(value))
  const ref = useRef<HTMLInputElement>(null)

  useEffect(() => { setDraft(String(value)) }, [value])
  useEffect(() => { if (editing) ref.current?.focus() }, [editing])

  if (!editing) {
    return (
      <span onDoubleClick={() => setEditing(true)} style={{ cursor: 'pointer' }}>
        {value}
      </span>
    )
  }

  const commit = () => {
    setEditing(false)
    const num = Number(draft)
    if (!isNaN(num) && num !== value) onSave(num)
  }

  return (
    <input
      ref={ref}
      type="number"
      value={draft}
      onChange={e => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={e => { if (e.key === 'Enter') commit(); if (e.key === 'Escape') { setDraft(String(value)); setEditing(false) } }}
      style={{ width: 80, padding: '0.2rem 0.4rem', fontSize: '0.9rem' }}
    />
  )
}

export default function Settings() {
  const qc = useQueryClient()
  const { data: config } = useQuery({ queryKey: ['config'], queryFn: () => api.get<any>('/config') })
  const { data: warehouses } = useQuery({ queryKey: ['warehouses'], queryFn: () => api.get<any[]>('/warehouses') })
  const { data: customerTypes } = useQuery({ queryKey: ['customer-types'], queryFn: () => api.get<any[]>('/customer-types') })
  const { data: customerGroups } = useQuery({ queryKey: ['customer-groups'], queryFn: () => api.get<any[]>('/customer-groups') })

  // Config form
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

  const configMutation = useMutation({
    mutationFn: () => api.put('/config', form),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['config'] }),
  })
  const set = (key: string, value: string) => setForm(f => ({ ...f, [key]: value }))

  // Warehouse CRUD
  const [whName, setWhName] = useState('')
  const [whAddress, setWhAddress] = useState('')
  const createWarehouse = useMutation({
    mutationFn: () => api.post('/warehouses', { name: whName, address: whAddress || null }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['warehouses'] }); setWhName(''); setWhAddress('') },
  })
  const updateWarehouse = useMutation({
    mutationFn: ({ id, name, address }: { id: number; name: string; address: string | null }) =>
      api.put(`/warehouses/${id}`, { name, address }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['warehouses'] }),
  })
  const reorderWarehouses = useMutation({
    mutationFn: (ids: (number | string)[]) => api.put('/warehouses/reorder', ids),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['warehouses'] }),
  })

  // Customer type CRUD
  const [ctName, setCtName] = useState('')
  const createType = useMutation({
    mutationFn: () => api.post('/customer-types', { name: ctName }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['customer-types'] }); setCtName('') },
  })
  const updateType = useMutation({
    mutationFn: ({ id, name }: { id: number; name: string }) => api.put(`/customer-types/${id}`, { name }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['customer-types'] }),
  })
  const reorderTypes = useMutation({
    mutationFn: (ids: (number | string)[]) => api.put('/customer-types/reorder', ids),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['customer-types'] }),
  })

  // Customer group CRUD
  const [cgTypeId, setCgTypeId] = useState(0)
  const [cgMarkup, setCgMarkup] = useState(0)
  const createGroup = useMutation({
    mutationFn: () => api.post('/customer-groups', { customer_type_id: cgTypeId, default_markup_pct: cgMarkup }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['customer-groups'] }); setCgMarkup(0) },
  })
  const updateGroup = useMutation({
    mutationFn: ({ id, default_markup_pct }: { id: number; default_markup_pct: number }) =>
      api.put(`/customer-groups/${id}`, { default_markup_pct }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['customer-groups'] }),
  })

  const typeName = (id: number) => customerTypes?.find((t: any) => t.id === id)?.name ?? '—'

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
      <div className="grid-2">
        {/* General Config */}
        <div className="card">
          <h2>General Configuration</h2>
          {fields.map(f => (
            <div key={f.key} className="form-group">
              <label>{f.label}</label>
              <input value={form[f.key] ?? ''} onChange={e => set(f.key, e.target.value)} />
            </div>
          ))}
          <button className="btn btn-primary mt-1" onClick={() => configMutation.mutate()} disabled={configMutation.isPending}>
            {configMutation.isPending ? 'Saving...' : 'Save Settings'}
          </button>
          {configMutation.isSuccess && <p style={{ color: 'var(--status-success)', marginTop: '0.5rem' }}>Settings saved</p>}
        </div>

        <div>
          {/* Warehouses */}
          <div className="card">
            <h2>Warehouses</h2>
            <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: '0.5rem' }}>
              Drag to reorder. Double-click to edit.
            </p>
            <div className="flex gap-1 mb-1" style={{ alignItems: 'flex-end' }}>
              <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
                <input value={whName} onChange={e => setWhName(e.target.value)} placeholder="Name" />
              </div>
              <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
                <input value={whAddress} onChange={e => setWhAddress(e.target.value)} placeholder="Address" />
              </div>
              <button className="btn btn-primary btn-sm" onClick={() => createWarehouse.mutate()} disabled={!whName}>Add</button>
            </div>
            <div className="table-wrap">
              <table>
                <thead><tr><th>Name</th><th>Address</th></tr></thead>
                {warehouses && warehouses.length > 0 ? (
                  <DragList keys={warehouses.map((w: any) => w.id)} onReorder={ids => reorderWarehouses.mutate(ids)}>
                    {warehouses.map((w: any) => (
                      <>
                        <td>
                          <EditableCell value={w.name} onSave={name => updateWarehouse.mutate({ id: w.id, name, address: w.address })} />
                        </td>
                        <td>
                          <EditableCell value={w.address ?? ''} onSave={address => updateWarehouse.mutate({ id: w.id, name: w.name, address: address || null })} />
                        </td>
                      </>
                    ))}
                  </DragList>
                ) : (
                  <tbody><tr><td colSpan={2} style={{ color: 'var(--text-muted)', textAlign: 'center' }}>No warehouses</td></tr></tbody>
                )}
              </table>
            </div>
          </div>

          {/* Customer Types */}
          <div className="card">
            <h2>Customer Types</h2>
            <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: '0.5rem' }}>
              Drag to reorder. Double-click to edit.
            </p>
            <div className="flex gap-1 mb-1" style={{ alignItems: 'flex-end' }}>
              <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
                <input value={ctName} onChange={e => setCtName(e.target.value)} placeholder="e.g. wholesale" />
              </div>
              <button className="btn btn-primary btn-sm" onClick={() => createType.mutate()} disabled={!ctName}>Add</button>
            </div>
            <div className="table-wrap">
              <table>
                <thead><tr><th>Name</th></tr></thead>
                {customerTypes && customerTypes.length > 0 ? (
                  <DragList keys={customerTypes.map((t: any) => t.id)} onReorder={ids => reorderTypes.mutate(ids)}>
                    {customerTypes.map((t: any) => (
                      <td key={t.id}>
                        <EditableCell value={t.name} onSave={name => updateType.mutate({ id: t.id, name })} />
                      </td>
                    ))}
                  </DragList>
                ) : (
                  <tbody><tr><td style={{ color: 'var(--text-muted)', textAlign: 'center' }}>No types</td></tr></tbody>
                )}
              </table>
            </div>
          </div>

          {/* Customer Groups (Pricing) */}
          <div className="card">
            <h2>Customer Groups (Pricing)</h2>
            <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: '0.5rem' }}>
              Double-click markup to edit.
            </p>
            <div className="flex gap-1 mb-1" style={{ alignItems: 'flex-end' }}>
              <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
                <select value={cgTypeId} onChange={e => setCgTypeId(Number(e.target.value))}>
                  <option value={0}>Customer type...</option>
                  {customerTypes?.map((t: any) => <option key={t.id} value={t.id}>{t.name}</option>)}
                </select>
              </div>
              <div className="form-group" style={{ width: 100, marginBottom: 0 }}>
                <input type="number" value={cgMarkup} onChange={e => setCgMarkup(Number(e.target.value))} placeholder="%" />
              </div>
              <button className="btn btn-primary btn-sm" onClick={() => createGroup.mutate()} disabled={!cgTypeId}>Add</button>
            </div>
            <div className="table-wrap">
              <table>
                <thead><tr><th>Customer Type</th><th>Markup %</th></tr></thead>
                <tbody>
                  {customerGroups?.map((g: any) => (
                    <tr key={g.id}>
                      <td>{typeName(g.customer_type_id)}</td>
                      <td>
                        <EditableNumber value={g.default_markup_pct} onSave={v => updateGroup.mutate({ id: g.id, default_markup_pct: v })} />
                      </td>
                    </tr>
                  ))}
                  {customerGroups?.length === 0 && (
                    <tr><td colSpan={2} style={{ color: 'var(--text-muted)', textAlign: 'center' }}>No groups</td></tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
