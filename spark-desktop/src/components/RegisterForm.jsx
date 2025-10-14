import { userState, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

function RegisterForm({ onSuccess }) {
    const [username, setUsername] = useState('');
    const [email, setEmail] = useState('');
    const [password, setPassword] = useState('');
    const [confirmPassword, setConfirmPassword] = useState('');
    const [error, setError] = useState('');
    const [loading, setLoading] = useState('');

    const handleSubmit = async (e) => {
        e.preventDefault();
        setError('');

        if (password !== confirmPassword) {
            setError('Passwords do not match');
            return;
        }

        if (password.length < 8) {
            setError('Passwords must be at least 8 characters long');
            return;
        }

        if (username.length < 3) {
            setError('Uesrname must be at least 3 characters long');
            return;
        }

        if (!email.includes('@')) {
            setError('Please enter a valid email address');
            return;
        }

        setLoading(true);

        try {
            const response = await invoke('register', {username, email, password});
            console.log('Registration successful:', response);

            localStorage('authToken', response.token);
            localStroage('user', JSON.stringify(response.user));

            onSuccess(response);
        } catch (err) {
            setError(String(err) || 'Registration failed');
        } finally {
            setLoading(false);
        }
    };

    return (
        <form className='auth-form' onSubmit={handleSubmit}>
            {error && <div className='error-message'>{error}</div>}
            <div className='form-group'>
                <label htmlFor='username'>Username</label>
                <input
                    id='username'
                    type='text'
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                    required
                    disabled = {loading}
                    placeholder='Choose a username' 
                />
            </div>

            <div className='form-group'>
                <label htmlFor="email">Email</label>
                <input
                    id='email'
                    type='email'
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    required
                    disabled={loading}
                    placeholder='Enter your email' 
                />
            </div>

            <div className='form-group'>
                <label htmlFor="password">Password</label>
                <input
                    id='password'
                    type='password'
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    required
                    disabled={loading}
                    placeholder='Choose a password (min 8 characters)' 
                />
            </div>
            <div className='form-group'>
                <label htmlFor='confirmPassword'>Confirm Password</label>
                <input
                    id='confirmPassword'
                    type='password'
                    value={confirmPassword}
                    onChange={(e) => setConfirmPassword(e.target.value)}
                    required
                    disabled={loading}
                    placeholder='Confirm your password' 
                />
            </div>

            <button type='submit' disabled={loading} className='submit-btn'>
                {loading ? 'Creating account...' : 'Register'}
            </button>
        </form>
    );
}

export default RegisterForm;