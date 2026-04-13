import { useState, useRef, useEffect } from 'react'
import { Routes, Route, Link, useLocation } from 'react-router-dom'
import { LayoutDashboard, Users, FileText, Calendar, ShoppingCart, MoreHorizontal, Settings as SettingsIcon, Package, Warehouse, UsersRound, CalendarCheck, Sun, Moon } from 'lucide-react'
import Dashboard from './pages/Dashboard'
import CustomerList from './pages/customers/CustomerList'
import CustomerForm from './pages/customers/CustomerForm'
import CustomerDetail from './pages/customers/CustomerDetail'
import ProductList from './pages/products/ProductList'
import ProductForm from './pages/products/ProductForm'
import InventoryView from './pages/products/InventoryView'
import InventoryReceive from './pages/products/InventoryReceive'
import SaleList from './pages/products/SaleList'
import SaleForm from './pages/products/SaleForm'
import SaleDetail from './pages/products/SaleDetail'
import CalendarView from './pages/services/CalendarView'
import QuoteList from './pages/services/QuoteList'
import QuoteForm from './pages/services/QuoteForm'
import QuoteDetail from './pages/services/QuoteDetail'
import BookingForm from './pages/services/BookingForm'
import BookingDetail from './pages/services/BookingDetail'
import TeamList from './pages/services/TeamList'
import DebtForm from './pages/services/DebtForm'
import Settings from './pages/Settings'

type Theme = 'light' | 'dark' | 'system'

function resolveIsDark(t: Theme) {
  return t === 'system'
    ? window.matchMedia('(prefers-color-scheme: dark)').matches
    : t === 'dark'
}

function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => {
    return (localStorage.getItem('theme') as Theme) ?? 'system'
  })
  const [isDark, setIsDark] = useState(() => resolveIsDark(
    (localStorage.getItem('theme') as Theme) ?? 'system'
  ))

  useEffect(() => {
    const dark = resolveIsDark(theme)
    setIsDark(dark)
    document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light')
    localStorage.setItem('theme', theme)

    if (theme === 'system') {
      const mq = window.matchMedia('(prefers-color-scheme: dark)')
      const handler = () => {
        const d = resolveIsDark('system')
        setIsDark(d)
        document.documentElement.setAttribute('data-theme', d ? 'dark' : 'light')
      }
      mq.addEventListener('change', handler)
      return () => mq.removeEventListener('change', handler)
    }
  }, [theme])

  const toggle = () => setTheme(isDark ? 'light' : 'dark')

  return { isDark, toggle }
}

function TopNav() {
  const location = useLocation()
  const [moreOpen, setMoreOpen] = useState(false)
  const moreRef = useRef<HTMLDivElement>(null)
  const { isDark, toggle } = useTheme()

  const isActive = (path: string) => {
    if (path === '/') return location.pathname === '/'
    return location.pathname.startsWith(path)
  }

  const moreRoutes = ['/products', '/inventory', '/teams', '/bookings']
  const moreIsActive = moreRoutes.some(p => location.pathname.startsWith(p))

  // Close dropdown on outside click
  useEffect(() => {
    if (!moreOpen) return
    const handler = (e: MouseEvent) => {
      if (moreRef.current && !moreRef.current.contains(e.target as Node)) setMoreOpen(false)
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [moreOpen])

  // Close on route change
  useEffect(() => { setMoreOpen(false) }, [location.pathname])

  return (
    <header className="topbar">
      <div className="topbar-left">
        <Link to="/" className="topbar-brand">CRM2</Link>
      </div>
      <nav className="topbar-nav">
        <Link to="/" className={isActive('/') ? 'active' : ''}><LayoutDashboard size={18} />Dashboard</Link>
        <Link to="/customers" className={isActive('/customers') ? 'active' : ''}><Users size={18} />Customers</Link>
        <Link to="/quotes" className={isActive('/quotes') || isActive('/debts') ? 'active' : ''}><FileText size={18} />Quotes</Link>
        <Link to="/sales" className={isActive('/sales') ? 'active' : ''}><ShoppingCart size={18} />Sales</Link>
        <Link to="/calendar" className={isActive('/calendar') ? 'active' : ''}><Calendar size={18} />Calendar</Link>

        <div className="topbar-more" ref={moreRef}>
          <button
            className={`topbar-more-btn ${moreIsActive ? 'active' : ''}`}
            onClick={() => setMoreOpen(o => !o)}
          >
            <MoreHorizontal size={18} />
            More
          </button>
          {moreOpen && (
            <div className="topbar-dropdown">
              <div className="topbar-dropdown-group">
                <div className="topbar-dropdown-label">Products</div>
                <Link to="/products"><Package size={17} />Catalog</Link>
                <Link to="/inventory"><Warehouse size={17} />Inventory</Link>
              </div>
              <div className="topbar-dropdown-group">
                <div className="topbar-dropdown-label">Teams</div>
                <Link to="/teams"><UsersRound size={17} />Teams</Link>
                <Link to="/bookings"><CalendarCheck size={17} />Bookings</Link>
              </div>
            </div>
          )}
        </div>
      </nav>
      <div className="topbar-right">
        <button className="theme-toggle" onClick={toggle} title="Toggle dark mode">
          {isDark ? <Sun size={16} /> : <Moon size={16} />}
        </button>
        <Link to="/settings" className={`topbar-util ${isActive('/settings') ? 'active' : ''}`}><SettingsIcon size={17} />Settings</Link>
      </div>
    </header>
  )
}

export default function App() {
  return (
    <div className="layout">
      <TopNav />
      <main className="main-content">
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/customers" element={<CustomerList />} />
          <Route path="/customers/new" element={<CustomerForm />} />
          <Route path="/customers/:id" element={<CustomerDetail />} />
          <Route path="/products" element={<ProductList />} />
          <Route path="/products/new" element={<ProductForm />} />
          <Route path="/inventory" element={<InventoryView />} />
          <Route path="/inventory/receive" element={<InventoryReceive />} />
          <Route path="/sales" element={<SaleList />} />
          <Route path="/sales/new" element={<SaleForm />} />
          <Route path="/sales/:id" element={<SaleDetail />} />
          <Route path="/calendar" element={<CalendarView />} />
          <Route path="/quotes" element={<QuoteList />} />
          <Route path="/quotes/new" element={<QuoteForm />} />
          <Route path="/quotes/:id" element={<QuoteDetail />} />
          <Route path="/bookings/new" element={<BookingForm />} />
          <Route path="/bookings/:id" element={<BookingDetail />} />
          <Route path="/teams" element={<TeamList />} />
          <Route path="/debts/new" element={<DebtForm />} />
          <Route path="/settings" element={<Settings />} />
        </Routes>
      </main>
    </div>
  )
}
