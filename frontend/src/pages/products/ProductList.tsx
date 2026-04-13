import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'

export default function ProductList() {
  const qc = useQueryClient()
  const { data: allProducts, isLoading } = useQuery({
    queryKey: ['products'],
    queryFn: () => api.get<any[]>('/products'),
  })
  const { data: warehouses } = useQuery({
    queryKey: ['warehouses'],
    queryFn: () => api.get<any[]>('/warehouses'),
  })

  const products = allProducts?.filter((p: any) => p.product_type === 'product') ?? []
  const services = allProducts?.filter((p: any) => p.product_type === 'service') ?? []

  const [whName, setWhName] = useState('')
  const [whAddress, setWhAddress] = useState('')
  const createWarehouse = useMutation({
    mutationFn: () => api.post('/warehouses', { name: whName, address: whAddress || null }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['warehouses'] }); setWhName(''); setWhAddress('') },
  })

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>Products & Services</h1>
        <div className="flex gap-1">
          <Link to="/products/new?type=product" className="btn btn-primary">Add Product</Link>
          <Link to="/products/new?type=service" className="btn">Add Service</Link>
        </div>
      </div>

      {isLoading ? <p>Loading...</p> : (
        <>
          <h2>Products</h2>
          <div className="table-wrap">
            <table>
              <thead>
                <tr><th>SKU</th><th>Name</th><th>Unit</th></tr>
              </thead>
              <tbody>
                {products.map((p: any) => (
                  <tr key={p.id}>
                    <td>{p.sku || '—'}</td>
                    <td>{p.name}</td>
                    <td>{p.unit}</td>
                  </tr>
                ))}
                {products.length === 0 && (
                  <tr><td colSpan={3} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>No products</td></tr>
                )}
              </tbody>
            </table>
          </div>

          <h2 className="mt-2">Services</h2>
          <div className="table-wrap">
            <table>
              <thead>
                <tr><th>Name</th><th>Description</th><th>Suggested Price</th></tr>
              </thead>
              <tbody>
                {services.map((s: any) => (
                  <tr key={s.id}>
                    <td>{s.name}</td>
                    <td>{s.description || '—'}</td>
                    <td>{s.suggested_price.toLocaleString()}</td>
                  </tr>
                ))}
                {services.length === 0 && (
                  <tr><td colSpan={3} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>No services</td></tr>
                )}
              </tbody>
            </table>
          </div>
        </>
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
          <button className="btn btn-primary" onClick={() => createWarehouse.mutate()} disabled={!whName}>Add</button>
        </div>
      </div>
      <div className="table-wrap">
        <table>
          <thead><tr><th>Name</th><th>Address</th></tr></thead>
          <tbody>
            {warehouses?.map((w: any) => (
              <tr key={w.id}><td>{w.name}</td><td>{w.address || '—'}</td></tr>
            ))}
            {warehouses?.length === 0 && (
              <tr><td colSpan={2} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>No warehouses</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  )
}
