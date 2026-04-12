import { useState, useMemo, useRef } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate } from 'react-router-dom'
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
function fmtLabel(d: Date) { return d.toLocaleDateString(undefined, { month: 'short', year: 'numeric' }) }
function pad(n: number) { return String(n).padStart(2, '0') }

const HOURS = Array.from({ length: 14 }, (_, i) => i + 7) // 7:00 - 20:00
const HOUR_HEIGHT = 60 // px per hour
const DAY_NAMES = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

export default function CalendarView() {
  const qc = useQueryClient()
  const nav = useNavigate()
  const [view, setView] = useState<'week' | 'month'>('week')
  const [currentDate, setCurrentDate] = useState(new Date())
  const [teamId, setTeamId] = useState<number | null>(null)

  // Drag state
  const [dragBookingId, setDragBookingId] = useState<number | null>(null)
  const [dropTarget, setDropTarget] = useState<string | null>(null)

  // Click-to-create state
  const [creating, setCreating] = useState<{ date: string; hour: number } | null>(null)

  const { data: teams } = useQuery({ queryKey: ['teams'], queryFn: () => api.get<any[]>('/teams') })
  const { data: customers } = useQuery({ queryKey: ['customers'], queryFn: () => api.get<any[]>('/customers') })

  const days = useMemo(() => view === 'week' ? getWeekDays(currentDate) : getMonthDays(currentDate), [view, currentDate])
  const start = fmt(days[0])
  const end = fmt(days[days.length - 1])

  const { data: bookings } = useQuery({
    queryKey: ['calendar', teamId, start, end],
    queryFn: () => api.get<any[]>(`/calendar?start=${start}&end=${end}${teamId ? `&team_id=${teamId}` : ''}`),
  })

  // Format a Date as local "YYYY-MM-DDTHH:MM" (no UTC conversion)
  const fmtLocal = (d: Date) =>
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`

  const moveMutation = useMutation({
    mutationFn: ({ id, newDate, newHour }: { id: number; newDate: string; newHour?: number }) => {
      const booking = bookings?.find((b: any) => b.id === id)
      if (!booking) return Promise.reject('Booking not found')
      const oldStart = new Date(booking.start_at)
      const oldEnd = new Date(booking.end_at)
      const duration = oldEnd.getTime() - oldStart.getTime()
      const hour = newHour ?? oldStart.getHours()
      const min = newHour != null ? 0 : oldStart.getMinutes()
      const newStart = new Date(`${newDate}T${pad(hour)}:${pad(min)}`)
      const newEnd = new Date(newStart.getTime() + duration)
      return api.put(`/bookings/${id}`, {
        start_at: fmtLocal(newStart),
        end_at: fmtLocal(newEnd),
      })
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['calendar'] }),
  })

  // Quick create
  const [qcTitle, setQcTitle] = useState('')
  const [qcTeam, setQcTeam] = useState(0)
  const [qcCustomer, setQcCustomer] = useState(0)
  const [qcDuration, setQcDuration] = useState(1)

  const quickCreate = useMutation({
    mutationFn: () => {
      if (!creating) return Promise.reject('No slot selected')
      const startAt = `${creating.date}T${pad(creating.hour)}:00`
      const endAt = `${creating.date}T${pad(creating.hour + qcDuration)}:00`
      return api.post('/bookings', {
        team_id: qcTeam,
        customer_id: qcCustomer,
        title: qcTitle,
        start_at: startAt,
        end_at: endAt,
      })
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['calendar'] })
      setCreating(null)
      setQcTitle('')
    },
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

  const getBookingsForDay = (day: Date) => {
    const ds = fmt(day)
    return bookings?.filter((b: any) => b.start_at.slice(0, 10) === ds) ?? []
  }

  const teamColor = (tid: number) => teams?.find((t: any) => t.id === tid)?.color ?? '#0d6efd'
  const teamName = (tid: number) => teams?.find((t: any) => t.id === tid)?.name ?? ''
  const today = fmt(new Date())

  // Week view: time grid
  const renderWeekView = () => {
    const weekDays = getWeekDays(currentDate)

    return (
      <div style={{ flex: 1, display: 'flex', overflow: 'hidden', borderRadius: 8, border: '1px solid var(--border)' }}>
        {/* Time gutter */}
        <div style={{ width: 50, flexShrink: 0, background: 'var(--bg)', borderRight: '1px solid var(--border)' }}>
          <div style={{ height: 32 }} /> {/* header spacer */}
          {HOURS.map(h => (
            <div key={h} style={{ height: HOUR_HEIGHT, fontSize: '0.7rem', color: 'var(--text-muted)', textAlign: 'right', padding: '0 4px', borderTop: '1px solid var(--border)' }}>
              {pad(h)}:00
            </div>
          ))}
        </div>

        {/* Day columns */}
        {weekDays.map((day, di) => {
          const dayStr = fmt(day)
          const isToday = dayStr === today
          const dayBookings = getBookingsForDay(day)

          return (
            <div key={di} style={{ flex: 1, display: 'flex', flexDirection: 'column', borderLeft: di > 0 ? '1px solid var(--border)' : undefined }}>
              {/* Day header */}
              <div style={{
                height: 32,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontWeight: 600,
                fontSize: '0.8rem',
                background: isToday ? '#e3f2fd' : 'var(--bg)',
                borderBottom: '1px solid var(--border)',
                color: isToday ? 'var(--primary)' : 'var(--text)',
              }}>
                {DAY_NAMES[di]} {day.getDate()}
              </div>

              {/* Time slots — layered: drop targets underneath, bookings on top */}
              <div style={{ flex: 1, position: 'relative', overflow: 'auto' }}>
                {/* Layer 1: Hour grid lines + drop targets (always interactive) */}
                {HOURS.map(h => (
                  <div
                    key={h}
                    onDoubleClick={() => {
                      setCreating({ date: dayStr, hour: h })
                      if (teams?.length) setQcTeam(teamId || teams[0].id)
                    }}
                    onDragOver={e => { e.preventDefault(); e.dataTransfer.dropEffect = 'move'; setDropTarget(`${dayStr}-${h}`) }}
                    onDragLeave={() => setDropTarget(null)}
                    onDrop={e => {
                      e.preventDefault()
                      setDropTarget(null)
                      if (dragBookingId !== null) {
                        moveMutation.mutate({ id: dragBookingId, newDate: dayStr, newHour: h })
                        setDragBookingId(null)
                      }
                    }}
                    style={{
                      height: HOUR_HEIGHT,
                      borderTop: '1px solid var(--border)',
                      cursor: 'pointer',
                      background: dropTarget === `${dayStr}-${h}` ? '#e8f0fe' :
                        creating?.date === dayStr && creating?.hour === h ? '#fff9c4' : undefined,
                      transition: 'background 0.1s',
                    }}
                  />
                ))}

                {/* Layer 2: Booking blocks — pointer-events disabled during drag so drops pass through */}
                <div style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  right: 0,
                  bottom: 0,
                  pointerEvents: dragBookingId !== null ? 'none' : 'auto',
                }}>
                  {dayBookings.map((b: any) => {
                    const bStart = new Date(b.start_at)
                    const bEnd = new Date(b.end_at)
                    const startHour = bStart.getHours() + bStart.getMinutes() / 60
                    const endHour = bEnd.getHours() + bEnd.getMinutes() / 60
                    const top = (startHour - HOURS[0]) * HOUR_HEIGHT
                    const height = Math.max((endHour - startHour) * HOUR_HEIGHT, 20)

                    return (
                      <div
                        key={b.id}
                        draggable
                        onDragStart={e => {
                          setDragBookingId(b.id)
                          e.dataTransfer.effectAllowed = 'move'
                        }}
                        style={{
                          position: 'absolute',
                          top,
                          left: 2,
                          right: 2,
                          height,
                          background: teamColor(b.team_id),
                          color: 'white',
                          borderRadius: 4,
                          padding: '0.15rem 0.3rem',
                          fontSize: '0.7rem',
                          cursor: 'grab',
                          overflow: 'hidden',
                          opacity: dragBookingId === b.id ? 0.4 : 1,
                          lineHeight: 1.25,
                          pointerEvents: 'auto',
                        }}
                      >
                        <Link to={`/bookings/${b.id}`} style={{ color: 'white', textDecoration: 'none' }}>
                          <div style={{ fontWeight: 600 }}>{b.title}</div>
                          <div style={{ opacity: 0.85, fontSize: '0.6rem' }}>
                            {b.start_at.slice(11, 16)}–{b.end_at.slice(11, 16)} · {teamName(b.team_id)}
                          </div>
                        </Link>
                      </div>
                    )
                  })}
                </div>
              </div>
            </div>
          )
        })}
      </div>
    )
  }

  // Month view: compact grid
  const renderMonthView = () => (
    <div style={{
      flex: 1,
      display: 'grid',
      gridTemplateColumns: 'repeat(7, 1fr)',
      gridTemplateRows: 'auto repeat(6, 1fr)',
      gap: '1px',
      background: 'var(--border)',
      border: '1px solid var(--border)',
      borderRadius: 8,
      overflow: 'hidden',
    }}>
      {DAY_NAMES.map(d => (
        <div key={d} style={{ background: 'var(--bg)', padding: '0.3rem', textAlign: 'center', fontWeight: 600, fontSize: '0.8rem', color: 'var(--text-muted)' }}>
          {d}
        </div>
      ))}
      {days.map((day, i) => {
        const dayStr = fmt(day)
        const isToday = dayStr === today
        const isCurrentMonth = day.getMonth() === currentDate.getMonth()
        const isDrop = dropTarget === dayStr
        const dayBookings = getBookingsForDay(day)

        return (
          <div
            key={i}
            onDoubleClick={() => {
              setCreating({ date: dayStr, hour: 9 })
              if (teams?.length) setQcTeam(teamId || teams[0].id)
            }}
            onDragOver={e => { e.preventDefault(); e.dataTransfer.dropEffect = 'move'; setDropTarget(dayStr) }}
            onDragLeave={() => setDropTarget(null)}
            onDrop={e => {
              e.preventDefault()
              setDropTarget(null)
              if (dragBookingId !== null) {
                moveMutation.mutate({ id: dragBookingId, newDate: dayStr })
                setDragBookingId(null)
              }
            }}
            style={{
              background: isDrop ? '#e8f0fe' : isToday ? '#fffde7' : 'var(--surface)',
              padding: '0.25rem',
              opacity: isCurrentMonth ? 1 : 0.35,
              overflow: 'auto',
              cursor: 'pointer',
              outline: isDrop ? '2px solid var(--primary)' : undefined,
              outlineOffset: '-2px',
            }}
          >
            <div style={{ fontSize: '0.72rem', fontWeight: isToday ? 700 : 500, color: isToday ? 'var(--primary)' : 'var(--text-muted)', marginBottom: '0.1rem' }}>
              {day.getDate()}
            </div>
            {dayBookings.map((b: any) => (
              <div
                key={b.id}
                draggable
                onDragStart={e => { e.stopPropagation(); setDragBookingId(b.id); e.dataTransfer.effectAllowed = 'move' }}
                onClick={e => e.stopPropagation()}
                style={{
                  background: teamColor(b.team_id),
                  color: 'white',
                  padding: '0.1rem 0.25rem',
                  borderRadius: 3,
                  marginBottom: 1,
                  fontSize: '0.65rem',
                  cursor: 'grab',
                  opacity: dragBookingId === b.id ? 0.4 : 1,
                }}
              >
                <Link to={`/bookings/${b.id}`} style={{ color: 'white', textDecoration: 'none' }} onClick={e => e.stopPropagation()}>
                  {b.title}
                </Link>
              </div>
            ))}
          </div>
        )
      })}
    </div>
  )

  return (
    <div style={{ margin: '-1.5rem', padding: '1rem', height: 'calc(100vh)', display: 'flex', flexDirection: 'column' }}>
      {/* Header */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.5rem', flexShrink: 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
          <h1 style={{ margin: 0, fontSize: '1.3rem' }}>Calendar</h1>
          <div className="flex gap-1" style={{ alignItems: 'center' }}>
            <button className="btn btn-sm" onClick={prev}>&larr;</button>
            <span style={{ fontWeight: 600, minWidth: 120, textAlign: 'center' }}>{fmtLabel(currentDate)}</span>
            <button className="btn btn-sm" onClick={next}>&rarr;</button>
            <button className="btn btn-sm" onClick={() => setCurrentDate(new Date())}>Today</button>
          </div>
        </div>
        <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
          <div className="tabs" style={{ marginBottom: 0 }}>
            <button className={`tab ${view === 'week' ? 'active' : ''}`} onClick={() => setView('week')}>Week</button>
            <button className={`tab ${view === 'month' ? 'active' : ''}`} onClick={() => setView('month')}>Month</button>
          </div>
          <select value={teamId ?? ''} onChange={e => setTeamId(e.target.value ? Number(e.target.value) : null)} style={{ width: 'auto', padding: '0.3rem 0.5rem' }}>
            <option value="">All Teams</option>
            {teams?.map((t: any) => <option key={t.id} value={t.id}>{t.name}</option>)}
          </select>
          <Link to="/bookings/new" className="btn btn-primary btn-sm">+ Booking</Link>
        </div>
      </div>

      {/* Calendar */}
      {view === 'week' ? renderWeekView() : renderMonthView()}

      {/* Quick create modal */}
      {creating && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.3)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 100 }}
          onClick={() => setCreating(null)}
        >
          <div className="card" style={{ width: 400, maxWidth: '90vw' }} onClick={e => e.stopPropagation()}>
            <h2 style={{ marginBottom: '0.75rem' }}>
              New Booking — {creating.date} at {pad(creating.hour)}:00
            </h2>
            <div className="form-group">
              <label>Title</label>
              <input value={qcTitle} onChange={e => setQcTitle(e.target.value)} autoFocus />
            </div>
            <div className="grid-2">
              <div className="form-group">
                <label>Team</label>
                <select value={qcTeam} onChange={e => setQcTeam(Number(e.target.value))}>
                  <option value={0}>Select...</option>
                  {teams?.map((t: any) => <option key={t.id} value={t.id}>{t.name}</option>)}
                </select>
              </div>
              <div className="form-group">
                <label>Customer</label>
                <select value={qcCustomer} onChange={e => setQcCustomer(Number(e.target.value))}>
                  <option value={0}>Select...</option>
                  {customers?.map((c: any) => <option key={c.id} value={c.id}>{c.name}</option>)}
                </select>
              </div>
            </div>
            <div className="form-group">
              <label>Duration (hours)</label>
              <select value={qcDuration} onChange={e => setQcDuration(Number(e.target.value))}>
                {[0.5, 1, 1.5, 2, 3, 4, 6, 8].map(h => <option key={h} value={h}>{h}h</option>)}
              </select>
            </div>
            <div className="flex gap-1 mt-1">
              <button
                className="btn btn-primary"
                onClick={() => quickCreate.mutate()}
                disabled={!qcTitle || !qcTeam || !qcCustomer || quickCreate.isPending}
              >
                {quickCreate.isPending ? 'Creating...' : 'Create'}
              </button>
              <button className="btn" onClick={() => setCreating(null)}>Cancel</button>
            </div>
            {quickCreate.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(quickCreate.error as Error).message}</p>}
          </div>
        </div>
      )}
    </div>
  )
}
