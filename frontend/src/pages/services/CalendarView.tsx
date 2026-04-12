import { useState, useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { api } from '../../api'

function getWeekDays(date: Date): Date[] {
  const day = date.getDay()
  const start = new Date(date)
  start.setDate(date.getDate() - (day === 0 ? 6 : day - 1))
  return Array.from({ length: 7 }, (_, i) => {
    const d = new Date(start)
    d.setDate(start.getDate() + i)
    return d
  })
}

function getMonthDays(date: Date): Date[] {
  const year = date.getFullYear()
  const month = date.getMonth()
  const first = new Date(year, month, 1)
  const startDay = first.getDay() === 0 ? 6 : first.getDay() - 1
  const start = new Date(first)
  start.setDate(1 - startDay)
  const days: Date[] = []
  for (let i = 0; i < 42; i++) {
    const d = new Date(start)
    d.setDate(start.getDate() + i)
    days.push(d)
  }
  return days
}

function fmt(d: Date) { return d.toISOString().slice(0, 10) }

export default function CalendarView() {
  const [view, setView] = useState<'week' | 'month'>('week')
  const [currentDate, setCurrentDate] = useState(new Date())
  const [teamId, setTeamId] = useState<number | null>(null)

  const { data: teams } = useQuery({ queryKey: ['teams'], queryFn: () => api.get<any[]>('/teams') })

  const days = useMemo(() => view === 'week' ? getWeekDays(currentDate) : getMonthDays(currentDate), [view, currentDate])
  const start = fmt(days[0])
  const end = fmt(days[days.length - 1])

  const { data: bookings } = useQuery({
    queryKey: ['calendar', teamId, start, end],
    queryFn: () => api.get<any[]>(`/calendar?start=${start}&end=${end}${teamId ? `&team_id=${teamId}` : ''}`),
  })

  const prev = () => {
    const d = new Date(currentDate)
    d.setDate(d.getDate() - (view === 'week' ? 7 : 30))
    setCurrentDate(d)
  }
  const next = () => {
    const d = new Date(currentDate)
    d.setDate(d.getDate() + (view === 'week' ? 7 : 30))
    setCurrentDate(d)
  }

  const dayNames = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

  const getBookingsForDay = (day: Date) => {
    const ds = fmt(day)
    return bookings?.filter((b: any) => b.start_at.slice(0, 10) === ds) ?? []
  }

  const teamColor = (tid: number) => teams?.find((t: any) => t.id === tid)?.color ?? '#0d6efd'

  return (
    <div>
      <div className="flex-between mb-2">
        <h1>Calendar</h1>
        <Link to="/bookings/new" className="btn btn-primary">New Booking</Link>
      </div>

      <div className="flex gap-2 mb-2">
        <div className="tabs">
          <button className={`tab ${view === 'week' ? 'active' : ''}`} onClick={() => setView('week')}>Week</button>
          <button className={`tab ${view === 'month' ? 'active' : ''}`} onClick={() => setView('month')}>Month</button>
        </div>
        <select
          value={teamId ?? ''}
          onChange={e => setTeamId(e.target.value ? Number(e.target.value) : null)}
          style={{ width: 'auto' }}
        >
          <option value="">All Teams</option>
          {teams?.map((t: any) => <option key={t.id} value={t.id}>{t.name}</option>)}
        </select>
        <div className="flex gap-1">
          <button className="btn btn-sm" onClick={prev}>&larr;</button>
          <button className="btn btn-sm" onClick={() => setCurrentDate(new Date())}>Today</button>
          <button className="btn btn-sm" onClick={next}>&rarr;</button>
        </div>
      </div>

      <div className="calendar-grid">
        {dayNames.map(d => <div key={d} className="calendar-cell-header">{d}</div>)}
        {days.map((day, i) => (
          <div key={i} className="calendar-cell" style={{
            opacity: day.getMonth() === currentDate.getMonth() ? 1 : 0.4,
          }}>
            <div style={{ fontSize: '0.8rem', fontWeight: 600, marginBottom: '0.2rem' }}>
              {day.getDate()}
            </div>
            {getBookingsForDay(day).map((b: any) => (
              <Link
                key={b.id}
                to={`/bookings/${b.id}`}
                className="calendar-event"
                style={{ background: teamColor(b.team_id), display: 'block' }}
              >
                {b.title}
              </Link>
            ))}
          </div>
        ))}
      </div>
    </div>
  )
}
