import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function QuoteList() {
  const { t } = useTranslation()
  const { data: quotes } = useQuery({
    queryKey: ['quotes'],
    queryFn: () => api.get<any[]>('/quotes'),
  })

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>{t('quotes.title')}</h1>
        <div className="flex gap-1">
          <Link to="/debts/new" className="btn">{t('quotes.quickDebt')}</Link>
          <Link to="/quotes/new" className="btn btn-primary">{t('quotes.newQuote')}</Link>
        </div>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr><th>{t('common.id')}</th><th>{t('common.versionId')}</th><th>{t('common.title')}</th><th>{t('common.status')}</th><th>{t('common.amount')}</th><th>{t('common.date')}</th></tr>
          </thead>
          <tbody>
            {quotes?.map((q: any) => (
              <tr key={q.id}>
                <td><Link to={`/quotes/${q.id}`}>#{q.id}</Link></td>
                <td><code title={q.version_id}>{q.version_id?.slice(0, 8)}</code></td>
                <td>
                  {q.title}
                  {q.is_debt ? <span className="badge badge-follow_up" style={{ marginLeft: '0.5rem' }}>{t('quotes.debt')}</span> : null}
                </td>
                <td><span className={`badge badge-${q.status}`}>{q.status}</span></td>
                <td>{q.total_amount.toLocaleString()}</td>
                <td>{new Date(q.created_at).toLocaleDateString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
