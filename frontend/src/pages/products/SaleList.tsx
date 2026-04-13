import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function SaleList() {
  const { t } = useTranslation()
  const { data: sales } = useQuery({
    queryKey: ['sales'],
    queryFn: () => api.get<any[]>('/sales'),
  })

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>{t('sales.title')}</h1>
        <Link to="/sales/new" className="btn btn-primary">{t('sales.newSale')}</Link>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr><th>{t('common.id')}</th><th>{t('common.versionId')}</th><th>{t('common.date')}</th><th>{t('common.total')}</th><th>{t('common.notes')}</th></tr>
          </thead>
          <tbody>
            {sales?.map((s: any) => (
              <tr key={s.id}>
                <td><Link to={`/sales/${s.id}`}>#{s.id}</Link></td>
                <td><code title={s.version_id}>{s.version_id?.slice(0, 8)}</code></td>
                <td>{new Date(s.sold_at).toLocaleDateString()}</td>
                <td><strong>{s.total_amount.toLocaleString()}</strong></td>
                <td>{s.notes || '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
