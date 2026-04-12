import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'

export default function ProductList() {
  const qc = useQueryClient()
  const { data: products, isLoading } = useQuery({
    queryKey: ['products'],
    queryFn: () => api.get<any[]>('/products'),
  })
  const { data: warehouses } = useQuery({
    queryKey: ['warehouses'],
    queryFn: () => api.get<any[]>('/warehouses'),
  })

  const [whName, setWhName] = useState('')
  const [whAddress, setWhAddress] = useState('')

  const createWarehouse = useMutation({
    mutationFn: () => api.post('/warehouses', { name: whName, address: whAddress || null }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['warehouses'] }); setWhName(''); setWhAddress('') },
  })

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>Products</h1>
        <Link to="/products/new" className="btn btn-primary">Add Product</Link>
      </div>

      {isLoading ? <p>Loading...</p> : (
        <table>
          <thead>
            <tr><th>SKU</th><th>Name</th><th>Unit</th></tr>
          </thead>
          <tbody>
            {products?.map((p: any) => (
              <tr key={p.id}>
                <td>{p.sku || '—'}</td>
                <td>{p.name}</td>
                <td>{p.unit}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <h2 className="mt-2">Warehouses</h2>
      <div className="card mb-1">
        <div className="flex gap-1" style={{ alignItems: 'flex-end' }}>
          <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
            <label>Name</label>
            <input value={whName} onChange={e => setWhName(e.target.value)} placeholder="Warehouse name" />
          </div>
          <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
            <label>Address</label>
            <input value={whAddress} onChange={e => setWhAddress(e.target.value)} placeholder="Address (optional)" />
          </div>
          <button
            className="btn btn-primary"
            onClick={() => createWarehouse.mutate()}
            disabled={!whName || createWarehouse.isPending}
          >
            Add Warehouse
          </button>
        </div>
      </div>
      <table>
        <thead><tr><th>Name</th><th>Address</th></tr></thead>
        <tbody>
          {warehouses?.map((w: any) => (
            <tr key={w.id}><td>{w.name}</td><td>{w.address || '—'}</td></tr>
          ))}
          {warehouses?.length === 0 && (
            <tr><td colSpan={2} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>No warehouses yet</td></tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
