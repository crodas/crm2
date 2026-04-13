import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

export default function TeamList() {
  const { t } = useTranslation()
  const qc = useQueryClient()
  const { data: teams } = useQuery({ queryKey: ['teams'], queryFn: () => api.get<any[]>('/teams') })
  const [selectedTeam, setSelectedTeam] = useState<number | null>(null)

  const { data: members } = useQuery({
    queryKey: ['team-members', selectedTeam],
    queryFn: () => api.get<any[]>(`/teams/${selectedTeam}/members`),
    enabled: !!selectedTeam,
  })

  const [teamName, setTeamName] = useState('')
  const [teamColor, setTeamColor] = useState('#5C7F63')
  const [memberName, setMemberName] = useState('')
  const [memberRole, setMemberRole] = useState('')

  const createTeam = useMutation({
    mutationFn: () => api.post('/teams', { name: teamName, color: teamColor }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['teams'] }); setTeamName('') },
  })

  const addMember = useMutation({
    mutationFn: () => api.post(`/teams/${selectedTeam}/members`, { name: memberName, role: memberRole || null }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['team-members', selectedTeam] }); setMemberName(''); setMemberRole('') },
  })

  return (
    <div>
      <h1>{t('teams.title')}</h1>
      <div className="grid-2">
        <div>
          <div className="card">
            <h2>{t('teams.createTeam')}</h2>
            <div className="grid-2">
              <div className="form-group">
                <label>{t('common.name')}</label>
                <input value={teamName} onChange={e => setTeamName(e.target.value)} />
              </div>
              <div className="form-group">
                <label>{t('common.color')}</label>
                <input type="color" value={teamColor} onChange={e => setTeamColor(e.target.value)} />
              </div>
            </div>
            <button className="btn btn-primary" onClick={() => createTeam.mutate()} disabled={!teamName}>{t('common.create')}</button>
          </div>

          <div className="table-wrap">
            <table>
              <thead><tr><th>{t('bookings.team')}</th><th>{t('common.color')}</th></tr></thead>
              <tbody>
                {teams?.map((tm: any) => (
                  <tr key={tm.id} onClick={() => setSelectedTeam(tm.id)} style={{ cursor: 'pointer' }}>
                    <td><strong>{tm.name}</strong></td>
                    <td><span style={{ display: 'inline-block', width: 16, height: 16, borderRadius: '50%', background: tm.color || '#ccc' }} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        <div>
          {selectedTeam ? (
            <>
              <div className="card">
                <h2>{t('teams.addMember')}</h2>
                <div className="grid-2">
                  <div className="form-group">
                    <label>{t('common.name')}</label>
                    <input value={memberName} onChange={e => setMemberName(e.target.value)} />
                  </div>
                  <div className="form-group">
                    <label>{t('common.role')}</label>
                    <input value={memberRole} onChange={e => setMemberRole(e.target.value)} />
                  </div>
                </div>
                <button className="btn btn-primary" onClick={() => addMember.mutate()} disabled={!memberName}>{t('common.add')}</button>
              </div>

              <h2>{t('teams.members')}</h2>
              <div className="table-wrap">
                <table>
                  <thead><tr><th>{t('common.name')}</th><th>{t('common.role')}</th></tr></thead>
                  <tbody>
                    {members?.map((m: any) => (
                      <tr key={m.id}><td>{m.name}</td><td>{m.role || '—'}</td></tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          ) : <p className="card">{t('teams.selectTeam')}</p>}
        </div>
      </div>
    </div>
  )
}
