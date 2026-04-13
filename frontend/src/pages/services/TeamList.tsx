import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../../api'

export default function TeamList() {
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
      <h1>Teams</h1>
      <div className="grid-2">
        <div>
          <div className="card">
            <h2>Create Team</h2>
            <div className="grid-2">
              <div className="form-group">
                <label>Name</label>
                <input value={teamName} onChange={e => setTeamName(e.target.value)} />
              </div>
              <div className="form-group">
                <label>Color</label>
                <input type="color" value={teamColor} onChange={e => setTeamColor(e.target.value)} />
              </div>
            </div>
            <button className="btn btn-primary" onClick={() => createTeam.mutate()} disabled={!teamName}>Create</button>
          </div>

          <table>
            <thead><tr><th>Team</th><th>Color</th></tr></thead>
            <tbody>
              {teams?.map((t: any) => (
                <tr key={t.id} onClick={() => setSelectedTeam(t.id)} style={{ cursor: 'pointer' }}>
                  <td><strong>{t.name}</strong></td>
                  <td><span style={{ display: 'inline-block', width: 16, height: 16, borderRadius: '50%', background: t.color || '#ccc' }} /></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <div>
          {selectedTeam ? (
            <>
              <div className="card">
                <h2>Add Member</h2>
                <div className="grid-2">
                  <div className="form-group">
                    <label>Name</label>
                    <input value={memberName} onChange={e => setMemberName(e.target.value)} />
                  </div>
                  <div className="form-group">
                    <label>Role</label>
                    <input value={memberRole} onChange={e => setMemberRole(e.target.value)} />
                  </div>
                </div>
                <button className="btn btn-primary" onClick={() => addMember.mutate()} disabled={!memberName}>Add</button>
              </div>

              <h2>Members</h2>
              <table>
                <thead><tr><th>Name</th><th>Role</th></tr></thead>
                <tbody>
                  {members?.map((m: any) => (
                    <tr key={m.id}><td>{m.name}</td><td>{m.role || '—'}</td></tr>
                  ))}
                </tbody>
              </table>
            </>
          ) : <p className="card">Select a team to view members</p>}
        </div>
      </div>
    </div>
  )
}
