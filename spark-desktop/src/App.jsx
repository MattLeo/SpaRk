import { useState, useEffect } from 'react'
import reactLogo from './assets/react.svg'
import viteLogo from '/vite.svg'
import Auth from './components/Auth'
import './App.css'

function App() {
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [user, setUser] = useState('');
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const token = localStorage.getItem('authToken');
    const userData = localStorage.getItem('user');

    if (token && userData) {
      setUser(JSON.parse(userData));
      setIsAuthenticated(true);
    }

    setLoading(false);
  });

  const handleAuthSuccess = (authResponse) => {
    setUser(authResponse.user);
    setIsAuthenticated(true);
  };

  const handleLogout = () => {
    localStorage.removeItem('authToken');
    localStorage.removeItem('user');
    setUser(null);
    setIsAuthenticated(false);
  };

  if (!isAuthenticated) {
    return <Auth onAuthSuccess={handleAuthSuccess} />;
  }

  return (
    <div className='app-container'>
      <header className='app-header'>
        <h1>SpaRk Chat</h1>
        <div className='user-info'>
          <span>Welcome, {user?.username}</span>
          <button onClick={handleLogout} className='logout-btn'>
            Logout
          </button>
        </div>
      </header>
      <main className='app-main'>
        <p> Chat Interface coming soon...</p>
      </main>
    </div>
  );
}

export default App;
