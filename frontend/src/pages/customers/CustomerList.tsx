import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function CustomerList() {
  const { t } = useTranslation()
  const [search, setSearch] = useState('')
  const { data: customers, isLoading } = useQuery({
    queryKey: ['customers', search],
    queryFn: () => api.get<any[]>(`/customers${search ? `?search=${encodeURIComponent(search)}` : ''}`),
  })
  const { data: types } = useQuery({
    queryKey: ['customer-types'],
    queryFn: () => api.get<any[]>('/customer-types'),
  })

  const typeName = (id: number) => types?.find((t: any) => t.id === id)?.name ?? ''

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>{t('customers.title')}</h1>
        <Link to="/customers/new" className="btn btn-primary">{t('customers.addCustomer')}</Link>
      </div>
      <input
        placeholder={t('customers.searchPlaceholder')}
        value={search}
        onChange={e => setSearch(e.target.value)}
        style={{ marginBottom: '1rem' }}
      />
      {isLoading ? <p>{t('common.loading')}</p> : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr><th>{t('common.name')}</th><th>{t('common.type')}</th><th>{t('common.phone')}</th><th>{t('common.email')}</th></tr>
            </thead>
            <tbody>
              {customers?.map((c: any) => (
                <tr key={c.id}>
                  <td><Link to={`/customers/${c.id}`}>{c.name}</Link></td>
                  <td><span className="badge badge-draft">{typeName(c.customer_type_id)}</span></td>
                  <td>{c.phone}</td>
                  <td>{c.email}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
