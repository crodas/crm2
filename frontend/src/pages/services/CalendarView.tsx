import { useState, useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate } from 'react-router-dom'
import { api } from '../../api'
import { useTranslation } from '../../i18n'

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
  const { t } = useTranslation()
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

  const teamColor = (tid: number) => teams?.find((t: any) => t.id === tid)?.color ?? '#5C7F63'
  const teamName = (tid: number) => teams?.find((t: any) => t.id === tid)?.name ?? ''
  const today = fmt(new Date())

  // Week view: time grid
  const renderWeekView = () => {
    const weekDays = getWeekDays(currentDate)

    return (
      <div className="cal-week-scroll">
      <div className="cal-week-grid" style={{ border: '1px solid var(--border-default)' }}>
        {/* Time gutter */}
        <div style={{ width: 50, flexShrink: 0, background: 'var(--bg-app)', borderRight: '1px solid var(--border-default)' }}>
          <div style={{ height: 32 }} />
          {HOURS.map(h => (
            <div key={h} style={{ height: HOUR_HEIGHT, fontSize: '0.7rem', color: 'var(--text-muted)', textAlign: 'right', padding: '0 4px', borderTop: '1px solid var(--border-default)' }}>
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
            <div key={di} style={{ flex: 1, display: 'flex', flexDirection: 'column', borderLeft: di > 0 ? '1px solid var(--border-default)' : undefined }}>
              {/* Day header */}
              <div style={{
                height: 32,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontWeight: 600,
                fontSize: '0.8rem',
                background: isToday ? 'var(--accent-primary-soft)' : 'var(--bg-app)',
                borderBottom: '1px solid var(--border-default)',
                color: isToday ? 'var(--accent-primary)' : 'var(--text-primary)',
              }}>
                {DAY_NAMES[di]} {day.getDate()}
              </div>

              {/* Time slots */}
              <div style={{ flex: 1, position: 'relative', overflow: 'auto' }}>
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
                      borderTop: '1px solid var(--border-default)',
                      cursor: 'pointer',
                      background: dropTarget === `${dayStr}-${h}` ? 'var(--accent-secondary-soft)' :
                        creating?.date === dayStr && creating?.hour === h ? 'var(--status-warning-soft)' : undefined,
                      transition: 'background 0.1s',
                    }}
                  />
                ))}

                {/* Booking blocks */}
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
      background: 'var(--border-default)',
      border: '1px solid var(--border-default)',
      borderRadius: 8,
      overflow: 'hidden',
    }}>
      {DAY_NAMES.map(d => (
        <div key={d} style={{ background: 'var(--bg-app)', padding: '0.3rem', textAlign: 'center', fontWeight: 600, fontSize: '0.8rem', color: 'var(--text-muted)' }}>
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
              background: isDrop ? 'var(--accent-secondary-soft)' : isToday ? 'var(--accent-primary-soft)' : 'var(--bg-surface)',
              padding: '0.25rem',
              opacity: isCurrentMonth ? 1 : 0.35,
              overflow: 'auto',
              cursor: 'pointer',
              outline: isDrop ? '2px solid var(--accent-primary)' : undefined,
              outlineOffset: '-2px',
            }}
          >
            <div style={{ fontSize: '0.72rem', fontWeight: isToday ? 700 : 500, color: isToday ? 'var(--accent-primary)' : 'var(--text-muted)', marginBottom: '0.1rem' }}>
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
    <div className="calendar-fullbleed">
      {/* Header */}
      <div className="cal-header">
        <div className="cal-header-left">
          <h1 style={{ margin: 0, fontSize: '1.3rem' }}>{t('calendar.title')}</h1>
          <div className="cal-nav">
            <button className="btn btn-sm" onClick={prev}>&larr;</button>
            <span style={{ fontWeight: 600, minWidth: 100, textAlign: 'center' }}>{fmtLabel(currentDate)}</span>
            <button className="btn btn-sm" onClick={next}>&rarr;</button>
            <button className="btn btn-sm" onClick={() => setCurrentDate(new Date())}>{t('calendar.today')}</button>
          </div>
        </div>
        <div className="cal-header-right">
          <div className="tabs" style={{ marginBottom: 0, borderBottom: 'none' }}>
            <button className={`tab ${view === 'week' ? 'active' : ''}`} onClick={() => setView('week')}>{t('calendar.week')}</button>
            <button className={`tab ${view === 'month' ? 'active' : ''}`} onClick={() => setView('month')}>{t('calendar.month')}</button>
          </div>
          <select value={teamId ?? ''} onChange={e => setTeamId(e.target.value ? Number(e.target.value) : null)} style={{ width: 'auto', padding: '0.3rem 0.5rem' }}>
            <option value="">{t('calendar.allTeams')}</option>
            {teams?.map((tm: any) => <option key={tm.id} value={tm.id}>{tm.name}</option>)}
          </select>
          <Link to="/bookings/new" className="btn btn-primary btn-sm">{t('calendar.addBooking')}</Link>
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
              {t('calendar.newBookingAt', { date: creating.date, time: `${pad(creating.hour)}:00` })}
            </h2>
            <div className="form-group">
              <label>{t('common.title')}</label>
              <input value={qcTitle} onChange={e => setQcTitle(e.target.value)} autoFocus />
            </div>
            <div className="grid-2">
              <div className="form-group">
                <label>{t('bookings.team')}</label>
                <select value={qcTeam} onChange={e => setQcTeam(Number(e.target.value))}>
                  <option value={0}>{t('common.select')}</option>
                  {teams?.map((tm: any) => <option key={tm.id} value={tm.id}>{tm.name}</option>)}
                </select>
              </div>
              <div className="form-group">
                <label>{t('bookings.customer')}</label>
                <select value={qcCustomer} onChange={e => setQcCustomer(Number(e.target.value))}>
                  <option value={0}>{t('common.select')}</option>
                  {customers?.map((c: any) => <option key={c.id} value={c.id}>{c.name}</option>)}
                </select>
              </div>
            </div>
            <div className="form-group">
              <label>{t('calendar.duration')}</label>
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
                {quickCreate.isPending ? t('common.creating') : t('common.create')}
              </button>
              <button className="btn" onClick={() => setCreating(null)}>{t('common.cancel')}</button>
            </div>
            {quickCreate.isError && <p style={{ color: 'red', marginTop: '0.5rem' }}>{(quickCreate.error as Error).message}</p>}
          </div>
        </div>
      )}
    </div>
  )
}
