import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './Chat.css';

function Chat({ user, onLogout }) {
    const [rooms, setRooms] = useState([]);
    const [currentRoom, setCurrentRoom] = useState(null);
    const [messages, setMessages] = useState({});
    const [messageInput, setMessageInput] = useState('');
    const [connected, setConnected] = useState(false);
    const [error, setError] = useState('');

    useEffect(() => {
        const token = localStorage.getItem('authToken');
        if (token) {
            connectWebSocket(token);
        }

        const unlistenMessage = listen('ws-message', (event) => {
            handleWebSocketMessage(event.playload);
        });

        const unlistenClosed = listen('ws-closed', (event) => {
            setConnected(false);
            setError('WebSocket connection closed');
        });

        const unlistenError = listen('ws-error', (event) => {
            setError(`Websocket Error: ${event.payload}`);
        });

        return () => {
            unlistenMessage.then(fn => fn());
            unlistenClosed.then(fn => fn());
            unlistenError.then(fn => fn());
        };
    }, []);

    const connectWebSocket = async (token) => {
        try {
            await invoke('connect_websocket', { token });
            setConnected(true);
            setError('');
        } catch (err) {
            setError(String(err));
        }
    };

    const handleWebSocketMessage = (msg) => {
        switch (msg.type) {
            case 'Authenticated':
                console.log('Authenticated:', msg);
                break;

            case 'RoomJoined':
                setRooms(prev => [...prev, {id: msg.room_id, name: msg.room_name }]);
                if (!currentRoom) {
                    setCurrentRoom(msg.room_id);
                }
                break;
            
            case 'RoomLeft':
                setRooms(prev => prev.filter(r => r.id !== msg.room_id));
                if (currentRoom === msg.room_id) {
                    if (rooms.length > 1) {
                        setCurrentRoom(rooms.at(-1));
                    } else {
                        setCurrentRoom(null);
                    }
                }
                break;

            case 'NewMessage':
                const roomId = msg.message.room_id;
                setMessages(prev => ({
                    ...prev,
                    [roomId]: [...(prev[roomId] || []), msg.message]
                }));
                break;
            
            case 'RoomHistory':
                setMessages(prev => ({
                    ...prev,
                    [msg.room_id]: msg.messages.reverse()
                }));
                break;

            case 'UserJoined':
                console.log(`User ${msg.username} joined ${msg.room_id}`);
                break;

            case 'UserLeft':
                console.log(`User ${msg.username} left ${msg.room_id}`);
                break;

            case 'Error':
                setError(msg.message);
                break;

            default:
                console.log('Unknown message type received:', msg);
        }
    };

    const joinRoom = async (roomId) => {
        try {
            await invoke('ws-join-room', { roomId });
            await invoke('ws-get-room-history', { roomId, limit: 100, offset: 0 });
        } catch (err) {
            setError(String(err));
        }
    };

    const leaveRoom = async (roomid) => {
        try {
            await invoke('ws-leave-room', { roomId });
        } catch (err) {
            setError(String(err));
        }
    };

    const sendMessage = async (e) => {
        e.preventDefault();
        if (!messageInput.trim() || !currentRoom) return;

        try {
            invoke('ws-send-message', {
                roomId: currentRoom,
                content: messageInput
            });
            setMessageInput('');
        } catch (err) {
            setError(String(err));
        }
    };

    const handleRoomSelect = (roomId) => {
        setCurrentRoom(roomId);
    };

    const currentMessages = currentRoom ? (messages[currentRoom] || []) : [];
    const currentRoomName = rooms.find(r => r.id === currentRoom)?.name || 'Select a room';

    return (
        <div className='chat-container'>
            <div className='chat-sidebar'>
                <div className='sidebar-header'>
                    <h2>Rooms</h2>
                    <button onClick={onLogout} className='logout-btn-small'>Logout</button>
                </div>

                <div className='room-list'>
                    {rooms.map(room => (
                        <div className={`room-item ${currentRoom === room.id ? 'active' : ''}`}
                            onClick={() => handleRoomSelect(room.id)}
                        >
                            <span className='room-name'>{room.name}</span>
                            <button
                                onClick={(e) => {
                                    e.stopPropagation();
                                    leaveRoom(room.id);
                                }}
                                className='leave-btn'
                        >×</button>
                        </div>
                    ))}
                </div>

                <div className='join-room-section'>
                    <input
                        type="text"
                        placeholder='Room ID to join...'
                        onKeyDown={(e) => {
                            if (e.key === 'Enter' && e.target.value.trim()) {
                                joinRoom(e.target.value.trim());
                                e.target.value = '';
                            }
                        }}
                    />
                </div>

                <div className='connection-status'>
                    <span className={connected ? 'connected': 'disconnected'}>
                        {connected ? '● Connected' : '○ Disconnected'}
                    </span>
                </div>
            </div>
            <div className='chat-main'>
                <div className='chat-header'>
                    <h2>{currentRoomName}</h2>
                    <span className='user-badge'>{user?.username}</span>
                </div>

                {error && (
                    <div className='error-banner'>
                        {error}
                        <button onClick={() => setError('')}>×</button>
                    </div>
                )}

                <div className='messages-container'>
                    {currentMessages.map((msg, idx) => (
                        <div key={idx} className='message'>
                            <div className='message-header'>
                                <span className='message-sender'>{message.sender_username}</span>
                                <span className='message-time'>{new Date(msg.sent_at).toLocaleTimeString()}</span>
                            </div>
                            <div className='message-content'>{msg.content}</div>
                        </div>
                    ))}
                    {currentMessages.length === 0 && currentRoom && (
                        <div className='no-messages'>No messages yet. Get the conversation started!</div>
                    )}
                    {!currentRoom && (
                        <div className='no-messages'>Select a room to start chatting</div>
                    )}
                </div>
                
                {currentRoom && (
                    <form onSubmit={sendMessage} className='message-input-form'>
                        <input
                            type='text'
                            value={messageInput}
                            onChange={(e) => setMessageInput(e.target.value)}
                            placeholder='Type a message...'
                            className='message-input'
                        />
                        <button type='submit' className='send-btn'>Send</button>
                    </form>
                )}
            </div>
        </div>
    );
}

export default Chat;