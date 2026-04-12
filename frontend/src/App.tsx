import { Routes, Route, Link, useLocation } from 'react-router-dom'
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

function Sidebar() {
  const location = useLocation()
  const isActive = (path: string) => location.pathname.startsWith(path) ? 'active' : ''

  return (
    <nav className="sidebar">
      <h2>CRM2</h2>
      <div className="sidebar-section">
        <h3>General</h3>
        <Link to="/" className={location.pathname === '/' ? 'active' : ''}>Dashboard</Link>
        <Link to="/customers" className={isActive('/customers')}>Customers</Link>
      </div>
      <div className="sidebar-section">
        <h3>Products</h3>
        <Link to="/products" className={isActive('/products')}>Products</Link>
        <Link to="/inventory" className={isActive('/inventory')}>Inventory</Link>
        <Link to="/sales" className={isActive('/sales')}>Sales</Link>
      </div>
      <div className="sidebar-section">
        <h3>Services</h3>
        <Link to="/calendar" className={isActive('/calendar')}>Calendar</Link>
        <Link to="/quotes" className={isActive('/quotes')}>Quotes</Link>
        <Link to="/bookings" className={isActive('/bookings')}>Bookings</Link>
        <Link to="/teams" className={isActive('/teams')}>Teams</Link>
      </div>
      <div className="sidebar-section">
        <h3>System</h3>
        <Link to="/settings" className={isActive('/settings')}>Settings</Link>
      </div>
    </nav>
  )
}

export default function App() {
  return (
    <div className="layout">
      <Sidebar />
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
