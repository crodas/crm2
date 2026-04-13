import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function ProductList() {
  const { t } = useTranslation()
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
        <h1>{t('products.title')}</h1>
        <div className="flex gap-1">
          <Link to="/products/new?type=product" className="btn btn-primary">{t('products.addProduct')}</Link>
          <Link to="/products/new?type=service" className="btn">{t('products.addService')}</Link>
        </div>
      </div>

      {isLoading ? <p>{t('common.loading')}</p> : (
        <>
          <h2>{t('products.productsSection')}</h2>
          <div className="table-wrap">
            <table>
              <thead>
                <tr><th>{t('products.sku')}</th><th>{t('common.name')}</th><th>{t('products.unit')}</th></tr>
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
                  <tr><td colSpan={3} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>{t('products.noProducts')}</td></tr>
                )}
              </tbody>
            </table>
          </div>

          <h2 className="mt-2">{t('products.servicesSection')}</h2>
          <div className="table-wrap">
            <table>
              <thead>
                <tr><th>{t('common.name')}</th><th>{t('common.description')}</th><th>{t('products.suggestedPrice')}</th></tr>
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
                  <tr><td colSpan={3} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>{t('products.noServices')}</td></tr>
                )}
              </tbody>
            </table>
          </div>
        </>
      )}

      <h2 className="mt-2">{t('products.warehousesSection')}</h2>
      <div className="card mb-1">
        <div className="flex gap-1" style={{ alignItems: 'flex-end' }}>
          <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
            <label>{t('common.name')}</label>
            <input value={whName} onChange={e => setWhName(e.target.value)} placeholder={t('products.warehouseName')} />
          </div>
          <div className="form-group" style={{ flex: 1, marginBottom: 0 }}>
            <label>{t('common.address')}</label>
            <input value={whAddress} onChange={e => setWhAddress(e.target.value)} placeholder={t('products.addressOptional')} />
          </div>
          <button className="btn btn-primary" onClick={() => createWarehouse.mutate()} disabled={!whName}>{t('common.add')}</button>
        </div>
      </div>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t('common.name')}</th><th>{t('common.address')}</th></tr></thead>
          <tbody>
            {warehouses?.map((w: any) => (
              <tr key={w.id}><td>{w.name}</td><td>{w.address || '—'}</td></tr>
            ))}
            {warehouses?.length === 0 && (
              <tr><td colSpan={2} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>{t('products.noWarehouses')}</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  )
}
