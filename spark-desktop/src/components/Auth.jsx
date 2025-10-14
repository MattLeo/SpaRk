import { useState } from 'react';
import LoginForm from './LoginForm';
import RegisterForm from './RegisterForm';
import './Auth.css';

function Auth({ onAuthSuccess }) {
    const [isLogin, setIsLogin] = useState(true);

    return (
        <div className='auth-container'>
            <div className='auth-box'>
                <h1 className='auth-title'>SpaRk</h1>
                <div className='auth-toggle'>
                    <button
                        className={isLogin ? 'active' : ''}
                        onClick={() => setIsLogin(true)}
                    >Login</button>
                    <button
                        className={!isLogin ? 'active' : ''}
                        onClick={() => setIsLogin(false)}
                    >Register</button>
                </div>
                {isLogin ? (
                    <LoginForm onSuccess={onAuthSuccess} />
                ) : (
                    <RegisterForm onSuccess={onAuthSuccess} />
                )}
            </div>
        </div>
    );
}

export default Auth;